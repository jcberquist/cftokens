//! This was cobbled together by looking at the syntect library and its examples
//! It does compile :)

extern crate serde;
extern crate syntect;
#[macro_use]
extern crate serde_json;
extern crate md5;
extern crate walkdir;

pub mod scopes;

use std::fs::{create_dir_all, File};
use std::io::prelude::*;
use std::io::{BufRead, BufReader};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::str::FromStr;

use scopes::{ContainerScope, DelimitedScope, CONTAINER_SCOPES, DELIMITED_SCOPES};
use serde::ser::{Serialize, SerializeSeq, SerializeStruct, Serializer};
use syntect::easy::ScopeRegionIterator;
use syntect::highlighting::ScopeSelector;
use syntect::parsing::{ParseState, Scope, ScopeStack, ScopeStackOp, SyntaxSet};
use walkdir::{DirEntry, WalkDir};

pub struct Token {
    text: String,
    scopes: Vec<String>,
}

pub struct Element {
    element_type: String,
    elements: Vec<Elements>,
    pop: String,
}

pub struct DelimitedElement {
    element_type: String,
    delimited_elements: Vec<Vec<Elements>>,
    pop: String,
    delimiter: String,
}

pub enum Elements {
    Token(Token),
    Element(Element),
    DelimitedElement(DelimitedElement),
}

impl Elements {
    pub fn add_element(&mut self, element: Elements) {
        match self {
            Elements::DelimitedElement(ref mut e) => {
                e.add_element(element);
            }
            Elements::Element(ref mut e) => {
                e.add_element(element);
            }
            Elements::Token(_e) => {}
        }
    }

    pub fn will_pop(&mut self, token: &str, scope_string: &String) -> bool {
        match self {
            Elements::DelimitedElement(ref mut e) => e.will_pop(token, scope_string),
            Elements::Element(ref mut e) => e.will_pop(token, scope_string),
            Elements::Token(_e) => false,
        }
    }
}

impl Serialize for Token {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(2))?;
        seq.serialize_element(&self.text)?;
        seq.serialize_element(&self.scopes)?;
        seq.end()
    }
}

impl Serialize for Elements {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Elements::DelimitedElement(e) => {
                let mut state = serializer.serialize_struct("Element", 2)?;
                state.serialize_field("type", &e.element_type)?;
                state.serialize_field("delimited_elements", &e.delimited_elements)?;
                state.end()
            }
            Elements::Element(e) => {
                let mut state = serializer.serialize_struct("Element", 2)?;
                state.serialize_field("type", &e.element_type)?;
                state.serialize_field("elements", &e.elements)?;
                state.end()
            }
            Elements::Token(e) => {
                // let mut seq = serializer.serialize_seq(Some(2))?;
                // seq.serialize_element(&e.text)?;
                // seq.serialize_element(&e.scopes)?;
                // seq.end()
                e.serialize(serializer)
            }
        }
    }
}

impl Element {
    pub fn new(element_type: &str, token: &str, scope_string: &String, end: &str) -> Element {
        // special string handling
        if element_type.contains("string") {
            let mut string_type = String::from(element_type);
            let script_count = scope_string.matches("source.cfml.script").count();
            if (scope_string.contains("meta.tag")
                && !scope_string.contains("meta.tag.cfml source.cfml.script")
                && script_count < 2)
                || (scope_string.contains("meta.class.declaration") && script_count < 2)
                || (scope_string.contains("meta.function.declaration")
                    && !scope_string.contains("meta.parameter"))
            {
                string_type.push_str("-tag");
            }
            Element {
                element_type: string_type,
                elements: Vec::new(),
                pop: scope_string.clone() + " " + end,
            }
        } else if element_type == "cftag" && token == "</" {
            Element {
                element_type: String::from("cftag-closed"),
                elements: Vec::new(),
                pop: scope_string.clone() + " " + end,
            }
        } else if element_type == "htmltag" && token == "</" {
            Element {
                element_type: String::from("htmltag-closed"),
                elements: Vec::new(),
                pop: scope_string.clone() + " " + end,
            }
        } else {
            Element {
                element_type: String::from(element_type),
                elements: Vec::new(),
                pop: scope_string.clone() + " " + end,
            }
        }
    }

    pub fn add_element(&mut self, element: Elements) {
        self.elements.push(element);
    }

    pub fn will_pop(&mut self, token: &str, scope_string: &String) -> bool {
        let pop = scope_string == &self.pop;
        if pop && token == "/>" {
            if self.element_type == "cftag" {
                self.element_type = String::from("cftag-selfclosed");
            } else if self.element_type == "htmltag" {
                self.element_type = String::from("htmltag-selfclosed");
            }
        }
        pop
    }
}

impl DelimitedElement {
    pub fn new(
        element_type: &str,
        scope_string: &String,
        delimiter: &str,
        end: &str,
    ) -> DelimitedElement {
        DelimitedElement {
            element_type: String::from(element_type),
            delimited_elements: vec![vec![]],
            pop: scope_string.clone() + " " + end,
            delimiter: scope_string.clone() + " " + delimiter,
        }
    }

    pub fn add_element(&mut self, element: Elements) {
        if let Elements::Token(token) = &element {
            if token.scopes.join(" ") == self.delimiter {
                self.delimited_elements.push(Vec::new());
                return;
            }
        }
        let i = self.delimited_elements.len() - 1;
        self.delimited_elements[i].push(element);
    }

    pub fn will_pop(&self, _token: &str, scope_string: &String) -> bool {
        scope_string == &self.pop
    }
}

fn is_ignored(entry: &DirEntry, cfm: &bool) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|s| {
            s.starts_with(".") && s.len() > 1
                || !(s.ends_with(".cfc") || (*cfm && s.ends_with(".cfm")))
                    && !entry.file_type().is_dir()
        })
        .unwrap_or(false)
}

fn write_to_file(path_string: &String, json: &String) {
    let path = Path::new(path_string);
    let display = path.display();

    match create_dir_all(&path.parent().unwrap()) {
        Err(e) => panic!("couldn't create directory to {}: {}", display, e),
        Ok(_) => {}
    }

    let mut file = match File::create(&path) {
        Err(e) => panic!("couldn't create {}: {}", display, e),
        Ok(file) => file,
    };

    match file.write_all(&json.as_bytes()) {
        Err(e) => panic!("couldn't write to {}: {}", display, e),
        Ok(_) => {}
    }
}

pub fn load_syntax_set() -> SyntaxSet {
    syntect::dumps::from_uncompressed_data(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/cftokens.packdump"
    )))
    .expect("valid embedded cftokens syntax pack")
}

fn cfml_syntax(ss: &SyntaxSet) -> &syntect::parsing::SyntaxReference {
    ss.find_syntax_by_scope(Scope::new("embedding.cfml").unwrap())
        .expect("CFML syntax (embedding.cfml) missing from pack")
}

struct ParsedLine {
    line: String,
    ops: Vec<(usize, ScopeStackOp)>,
}

fn parse_error(path: &str, detail: impl std::fmt::Display) -> String {
    format!("Unable to parse {}: {}", path, detail)
}

fn emit_parse_warnings(path: &str, line_number: usize, warnings: &[String]) {
    if warnings.is_empty() || std::env::var_os("CFTOKENS_DEBUG").is_none() {
        return;
    }
    for warning in warnings {
        eprintln!("{}:{}: {}", path, line_number, warning);
    }
}

fn check_ops(ops: &[(usize, ScopeStackOp)], line: &str) -> Result<(), String> {
    let mut last = 0;
    for &(pos, _) in ops {
        if pos < last || pos > line.len() || !line.is_char_boundary(pos) {
            return Err("inconsistent scope operations".to_string());
        }
        last = pos;
    }
    Ok(())
}

fn apply_replayed(
    parsed: &mut [ParsedLine],
    replayed: Vec<Vec<(usize, ScopeStackOp)>>,
) -> Result<(), String> {
    // Concatenated replays: syntect's handle_fail does flushed_ops.extend(...),
    // so if two cross-line fails resolve within a single parse_line call,
    // replayed.len() can exceed the number of buffered lines and suffix
    // mapping would misattribute ops. This is an upstream API limitation
    // (needs per-chunk offsets); treat overflow as a parse error.
    let start = parsed
        .len()
        .checked_sub(replayed.len())
        .ok_or_else(|| "replayed more lines than parsed".to_string())?;
    for (i, replayed_ops) in replayed.into_iter().enumerate() {
        check_ops(&replayed_ops, &parsed[start + i].line)?;
        parsed[start + i].ops = replayed_ops;
    }
    Ok(())
}

fn parse_all_lines(ss: &SyntaxSet, path: &str) -> Result<Vec<ParsedLine>, String> {
    let syntax = cfml_syntax(ss);
    let mut state = ParseState::new(syntax);

    let mut reader: Box<dyn BufRead> = match path {
        "-" => Box::new(BufReader::new(std::io::stdin())),
        _ => Box::new(BufReader::new(
            File::open(path).map_err(|e| parse_error(path, e))?,
        )),
    };

    let mut parsed: Vec<ParsedLine> = Vec::new();
    let mut line = String::new();

    while reader
        .read_line(&mut line)
        .map_err(|e| parse_error(path, e))?
        > 0
    {
        let output = state
            .parse_line(&line, ss)
            .map_err(|e| parse_error(path, e))?;
        emit_parse_warnings(path, parsed.len() + 1, &output.warnings);
        if !output.replayed.is_empty() {
            apply_replayed(&mut parsed, output.replayed).map_err(|e| parse_error(path, e))?;
        }
        check_ops(&output.ops, &line).map_err(|e| parse_error(path, e))?;
        parsed.push(ParsedLine {
            line: line.clone(),
            ops: output.ops,
        });
        line.clear();
    }

    Ok(parsed)
}

pub fn tokenize(ss: &SyntaxSet, path: String) -> Result<String, String> {
    let parsed = parse_all_lines(ss, &path)?;
    let mut stack = ScopeStack::new();
    let mut tokens: Vec<Token> = Vec::new();

    for record in &parsed {
        for (s, op) in ScopeRegionIterator::new(&record.ops, &record.line) {
            stack.apply(op).map_err(|e| parse_error(&path, e))?;
            if s.is_empty() {
                continue;
            }

            let scopes: Vec<String> = stack
                .as_slice()
                .iter()
                .skip(1)
                .map(|e| e.build_string())
                .collect();

            tokens.push(Token {
                text: s.to_string(),
                scopes,
            });
        }
    }

    Ok(format!("{}", json!(tokens)))
}

pub fn parse(ss: &SyntaxSet, path: String) -> Result<String, String> {
    let punctuation_selector = ScopeSelector::from_str("punctuation").unwrap();

    let mut delimited_scopes = Vec::new();
    for (name, start, delimiter, end) in DELIMITED_SCOPES.iter() {
        delimited_scopes.push(DelimitedScope::new(name, start, delimiter, end))
    }

    let mut container_scopes = Vec::new();
    for (name, start, end) in CONTAINER_SCOPES.iter() {
        container_scopes.push(ContainerScope::new(name, start, end))
    }

    let parsed = parse_all_lines(ss, &path)?;
    let mut stack = ScopeStack::new();

    let mut token_stack: Vec<Elements> = Vec::new();
    let cfscript = Element::new("cfml", "", &String::new(), "");
    token_stack.push(Elements::Element(cfscript));

    for record in &parsed {
        for (s, op) in ScopeRegionIterator::new(&record.ops, &record.line) {
            stack.apply(op).map_err(|e| parse_error(&path, e))?;
            if s.is_empty() {
                continue;
            }

            let stack_index = token_stack.len() - 1;
            let scopes: Vec<String> = stack
                .as_slice()
                .iter()
                .skip(1)
                .map(|e| e.build_string())
                .collect();
            let scope_string = scopes.join(" ");

            if token_stack[stack_index].will_pop(&s, &scope_string) {
                if let Some(last) = token_stack.pop() {
                    match token_stack.last_mut() {
                        Some(parent) => parent.add_element(last),
                        None => return Err(format!("Unable to parse {}", path)),
                    }
                    continue;
                } else {
                    return Err(format!("Unable to parse {}", path));
                }
            }

            if punctuation_selector.does_match(stack.as_slice()).is_some() {
                let n = scopes.len() - 1;
                let base_scope_string = scopes[..n].join(" ");
                let mut matched = false;

                for ds in delimited_scopes.iter() {
                    if scope_string.ends_with(ds.start) {
                        let de = DelimitedElement::new(
                            ds.name,
                            &base_scope_string,
                            ds.delimiter,
                            ds.end,
                        );
                        token_stack.push(Elements::DelimitedElement(de));
                        matched = true;
                        break;
                    }
                }
                if matched {
                    continue;
                }

                for cs in container_scopes.iter() {
                    if scope_string.ends_with(cs.start) {
                        let e = Element::new(cs.name, &s, &base_scope_string, cs.end);
                        token_stack.push(Elements::Element(e));
                        matched = true;
                        break;
                    }
                }
                if matched {
                    continue;
                }
            }

            let mut text = s.to_owned();
            if !scope_string.ends_with("cfformat.ignore.cfml") {
                text = text.replace("\r", "")
            }

            let token = Token { text: text, scopes };
            token_stack[stack_index].add_element(Elements::Token(token));
        }
    }

    if token_stack.len() != 1 {
        return Err(format!("Unable to parse {}", path));
    }

    Ok(format!("{}", json!(token_stack.last().unwrap())))
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic during parse".to_string()
    }
}

pub fn tokenize_file(ss: &SyntaxSet, cmd: &String, path: String) -> Result<String, String> {
    let path_for_err = path.clone();
    match catch_unwind(AssertUnwindSafe(|| {
        if cmd == "tokenize" {
            tokenize(ss, path)
        } else {
            parse(ss, path)
        }
    })) {
        Ok(result) => result,
        Err(payload) => Err(parse_error(&path_for_err, panic_message(payload))),
    }
}

pub fn tokenize_dir(
    ss: &SyntaxSet,
    cmd: &String,
    src_path: String,
    target_path: String,
    cfm: bool,
) -> String {
    let walker = WalkDir::new(&src_path).into_iter();
    let mut output = Vec::new();

    for entry in walker.filter_entry(|e| !is_ignored(e, &cfm)) {
        let entry = entry.unwrap();
        if entry.file_type().is_file() {
            let entry_path = entry.path().to_str().unwrap();
            let output_path = entry_path.replace(&src_path, &target_path);
            match tokenize_file(&ss, cmd, entry_path.to_string()) {
                Ok(json) => {
                    let output_file_path = output_path
                        .replace(".cfc", ".json")
                        .replace(".cfm", ".json");
                    write_to_file(&output_file_path, &json);
                    output.push((entry_path.to_string(), output_file_path));
                }
                Err(e) => {
                    eprintln!("{}", e);
                    let output_file_path = output_path
                        .replace(".cfc", ".error")
                        .replace(".cfm", ".error");
                    write_to_file(&output_file_path, &e);
                }
            }
        }
    }

    format!("{}", json!(output))
}

pub fn tokenize_manifest(ss: &SyntaxSet, cmd: &String, src_path: String) -> String {
    let base_path = Path::new(&src_path).parent().unwrap();
    let mut output = Vec::new();

    let f = File::open(&src_path).unwrap();
    let reader = BufReader::new(f);

    for line in reader.lines() {
        let path = line.unwrap();
        let hash = &format!("{:x}", md5::compute(path.as_bytes()));
        let target_path = base_path.join(hash);

        if path.ends_with(".cfc") || path.ends_with(".cfm") {
            let mut output_file_path = target_path.to_str().unwrap().to_string();
            match tokenize_file(&ss, cmd, path.to_string()) {
                Ok(json) => {
                    output_file_path.push_str(".json");
                    write_to_file(&output_file_path, &json);
                    output.push((path, output_file_path));
                }
                Err(e) => {
                    eprintln!("{}", e);
                    output_file_path.push_str(".error");
                    write_to_file(&output_file_path, &e);
                }
            }
        } else {
            eprintln!("{} is not a valid path.", path);
        }
    }

    format!("{}", json!(output))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_replayed_errors_when_replay_exceeds_parsed_lines() {
        let mut parsed = vec![ParsedLine {
            line: "a\n".into(),
            ops: vec![],
        }];
        let replayed = vec![vec![], vec![]];
        assert!(apply_replayed(&mut parsed, replayed).is_err());
    }

    #[test]
    fn apply_replayed_replaces_suffix() {
        let mut parsed = vec![
            ParsedLine {
                line: "a\n".into(),
                ops: vec![],
            },
            ParsedLine {
                line: "b\n".into(),
                ops: vec![],
            },
        ];
        let replayed = vec![vec![(0, ScopeStackOp::Noop)]];
        apply_replayed(&mut parsed, replayed).unwrap();
        assert!(parsed[0].ops.is_empty());
        assert_eq!(parsed[1].ops.len(), 1);
    }
}
