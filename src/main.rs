extern crate cftokens;
extern crate syntect;

use std::env;
use std::process;
use syntect::parsing::SyntaxSet;

struct DirConfig {
    cmd: String,
    src_path: String,
    target_path: String,
    cfm: bool,
}

struct FileConfig {
    cmd: String,
    src_path: String,
}

struct ManifestConfig {
    cmd: String,
    src_path: String,
}

enum Config {
    File(FileConfig),
    Dir(DirConfig),
    Manifest(ManifestConfig),
}

impl Config {
    pub fn new(mut args: std::env::Args) -> Result<Config, &'static str> {
        args.next();

        let cmd = match args.next() {
            Some(arg) => {
                if arg == "parse" || arg == "tokenize" {
                    arg
                } else {
                    return Err("Please specify parse or tokenize.");
                }
            }
            None => return Err("Please provide a source path."),
        };

        let src_path = match args.next() {
            Some(arg) => arg,
            None => return Err("Please provide a source path."),
        };

        match args.next() {
            Some(target) => {
                // we have source and target dir
                match args.next() {
                    Some(cfm) => {
                        // we have source and target dir
                        let config = DirConfig {
                            cmd,
                            src_path,
                            target_path: target,
                            cfm: cfm == "--cfm",
                        };
                        Ok(Config::Dir(config))
                    }
                    None => {
                        let config = DirConfig {
                            cmd,
                            src_path,
                            target_path: target,
                            cfm: false,
                        };
                        Ok(Config::Dir(config))
                    }
                }
            }
            None => {
                if src_path.ends_with(".cfc") || src_path.ends_with(".cfm") {
                    let config = FileConfig { cmd, src_path };
                    Ok(Config::File(config))
                } else {
                    let config = ManifestConfig { cmd, src_path };
                    Ok(Config::Manifest(config))
                }
            }
        }
    }
}

fn main() {
    let config = Config::new(env::args()).unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });

    let ss = SyntaxSet::load_defaults_newlines();

    let json = match config {
        Config::File(config) => match cftokens::tokenize_file(&ss, &config.cmd, config.src_path) {
            Ok(json) => json,
            Err(e) => e,
        },
        Config::Dir(config) => cftokens::tokenize_dir(
            &ss,
            &config.cmd,
            config.src_path,
            config.target_path,
            config.cfm,
        ),
        Config::Manifest(config) => cftokens::tokenize_manifest(&ss, &config.cmd, config.src_path),
    };

    print!("{}", json);
}
