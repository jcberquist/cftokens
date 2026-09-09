extern crate cftokens;
extern crate serde_json;

use serde_json::Value;
use std::fs;
use std::path::PathBuf;

fn write_fixture(name: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "cftokens-{}-{}-{}.cfc",
        name,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::write(&path, source).unwrap();
    path
}

fn parse_json(source: &str) -> Value {
    let ss = cftokens::load_syntax_set();
    let path = write_fixture("parse", source);
    let json = cftokens::parse(&ss, path.to_string_lossy().into_owned()).unwrap();
    let _ = fs::remove_file(&path);
    serde_json::from_str(&json).unwrap()
}

fn tokenize_json(source: &str) -> Value {
    let ss = cftokens::load_syntax_set();
    let path = write_fixture("tokenize", source);
    let json = cftokens::tokenize(&ss, path.to_string_lossy().into_owned()).unwrap();
    let _ = fs::remove_file(&path);
    serde_json::from_str(&json).unwrap()
}

fn collect_types(value: &Value, types: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(t)) = map.get("type") {
                types.push(t.clone());
            }
            for child in map.values() {
                collect_types(child, types);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_types(item, types);
            }
        }
        _ => {}
    }
}

fn last_scopes(tokens: &Value) -> Vec<String> {
    tokens
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|token| {
            token.as_array().and_then(|pair| {
                pair.get(1)
                    .and_then(|scopes| scopes.as_array())
                    .and_then(|scopes| scopes.last())
                    .and_then(|scope| scope.as_str())
                    .map(|s| s.to_string())
            })
        })
        .collect()
}

#[test]
fn script_component_uses_cfml_entry_syntax() {
    let tokens = tokenize_json("component {}\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes.iter().any(|s| s == "storage.type.class.cfml"),
        "expected component keyword, got {:?}",
        scopes
    );
}

#[test]
fn doc_comment_then_component_is_recognized() {
    let tokens = tokenize_json("/** documentation */ component {}\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes.iter().any(|s| s == "storage.type.class.cfml"),
        "expected component after documentation comment, got {:?}",
        scopes
    );
}

#[test]
fn structured_parse_emits_array_struct_and_call() {
    let parsed = parse_json("component { x = [1, 2]; y = {a: 1}; z = foo(1, 2); }\n");
    let mut types = Vec::new();
    collect_types(&parsed, &mut types);
    for required in ["array", "struct", "function-call"] {
        assert!(
            types.iter().any(|t| t == required),
            "missing {} in {:?}",
            required,
            types
        );
    }
}

#[test]
fn ordered_struct_is_not_an_array() {
    let parsed = parse_json("component { x = [a: 1, b: 2]; }\n");
    let mut types = Vec::new();
    collect_types(&parsed, &mut types);
    assert!(types.iter().any(|t| t == "struct"));
    assert!(!types.iter().any(|t| t == "array"));
}

#[test]
fn ignore_regions_use_cfformat_scope() {
    let tokens = tokenize_json("// cfformat-ignore-start\nvar x = 1;\n// cfformat-ignore-end\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes.iter().any(|s| s == "cfformat.ignore.cfml"),
        "expected ignore scope, got {:?}",
        scopes
    );
}

#[test]
fn tag_ignore_regions_use_cfformat_scope() {
    let tokens = tokenize_json(
        "<!--- cfformat-ignore-start --->\n<cfset x = 1>\n<!--- cfformat-ignore-end --->\n",
    );
    let scopes = last_scopes(&tokens);
    assert!(
        scopes.iter().any(|s| s == "cfformat.ignore.cfml"),
        "expected tag ignore scope, got {:?}",
        scopes
    );
}

#[test]
fn cfoutput_parses_attributes_and_quoted_gt() {
    let tokens = tokenize_json("<cfoutput query=\"q\" startrow=\">\">#x#</cfoutput>\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes
            .iter()
            .any(|s| s.starts_with("entity.other.attribute-name")),
        "expected cfoutput attributes, got {:?}",
        scopes
    );
}

#[test]
fn word_operator_continues_statement() {
    let tokens = tokenize_json("component {\nx = 1\nand 2;\n}\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes
            .iter()
            .any(|s| s == "keyword.operator.logical.binary.cfml"),
        "expected continued word operator, got {:?}",
        scopes
    );
}

#[test]
fn multiline_arrow_function_is_a_function_not_a_group() {
    let parsed = parse_json("component {\ncb = (x)\n=> x;\n}\n");
    let mut types = Vec::new();
    collect_types(&parsed, &mut types);
    assert!(
        types.iter().any(|t| t == "function-parameters"),
        "expected arrow function parameters after replay, got {:?}",
        types
    );
}

#[test]
fn bom_script_file_is_parsed_as_script() {
    let tokens = tokenize_json("\u{FEFF}component {}\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes.iter().any(|s| s == "storage.type.class.cfml"),
        "expected component keyword after BOM, got {:?}",
        scopes
    );
}

#[test]
fn bom_is_emitted_as_leading_bom_token() {
    let tokens = tokenize_json("\u{FEFF}component {}\n");
    let first = tokens
        .as_array()
        .unwrap()
        .first()
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(first[0], "\u{FEFF}");
    assert_eq!(first[1][0], "bom");
}

#[test]
fn bom_is_preserved_in_structured_parse() {
    let parsed = parse_json("\u{FEFF}component {}\n");
    let elements = parsed.get("elements").unwrap().as_array().unwrap();
    let first = elements.first().unwrap().as_array().unwrap();
    assert_eq!(first[0], "\u{FEFF}");
    assert_eq!(first[1][0], "bom");
}

#[test]
fn bom_tags_file_still_parses_tags() {
    let tokens = tokenize_json("\u{FEFF}<cfoutput query=\"q\">#x#</cfoutput>\n");
    let first = tokens
        .as_array()
        .unwrap()
        .first()
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(first[0], "\u{FEFF}");
    assert_eq!(first[1][0], "bom");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes
            .iter()
            .any(|s| s.starts_with("entity.other.attribute-name")),
        "expected cfoutput attributes after BOM, got {:?}",
        scopes
    );
}

#[test]
fn interface_after_bom_or_doc_comment_is_recognized() {
    for source in [
        "\u{FEFF}interface I {}\n",
        "/** documentation */ interface I {}\n",
    ] {
        let tokens = tokenize_json(source);
        let scopes = last_scopes(&tokens);
        assert!(
            scopes.iter().any(|s| s == "storage.type.interface.cfml"),
            "expected interface keyword in {:?}, got {:?}",
            source.trim(),
            scopes
        );
    }
}

#[test]
fn missing_file_returns_err_instead_of_panicking() {
    let ss = cftokens::load_syntax_set();
    let result = cftokens::tokenize(&ss, "/nonexistent/cftokens-missing.cfc".into());
    assert!(result.is_err(), "expected Err, got {:?}", result);
    let result = cftokens::parse(&ss, "/nonexistent/cftokens-missing.cfc".into());
    assert!(result.is_err(), "expected Err, got {:?}", result);
}

#[test]
fn tokenize_file_returns_err_for_missing_file() {
    let ss = cftokens::load_syntax_set();
    let cmd = "tokenize".to_string();
    let result = cftokens::tokenize_file(&ss, &cmd, "/nonexistent/cftokens-missing.cfc".into());
    assert!(result.is_err(), "expected Err, got {:?}", result);
}

#[test]
fn unresolved_branch_point_expiry_does_not_panic() {
    let mut source = String::from("component {\nx = [\n");
    for i in 0..130 {
        source.push_str(&format!("{}\n", i));
    }
    source.push_str("];\n}\n");
    let tokens = tokenize_json(&source);
    assert!(tokens.as_array().unwrap().len() > 1);
}

#[test]
fn json_script_embeds_json_syntax() {
    let tokens = tokenize_json("<script type=\"application/json\">{ \"a\": 1 }</script>\n");
    let scopes = last_scopes(&tokens);
    assert!(
        scopes.iter().any(|s| s.ends_with(".json")),
        "expected JSON script content to embed the JSON syntax, got {:?}",
        scopes
    );
}
