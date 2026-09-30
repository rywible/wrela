use std::{env, fs, path::PathBuf};

fn main() {
    let out = PathBuf::from(env::var("OUT_DIR").expect("Cargo OUT_DIR"));
    let fragments = [
        ("// @shared-type-syntax", "src/type-syntax.lalrpop.inc"),
        ("// @shared-terminals", "src/terminals.lalrpop.inc"),
    ];
    for (_, path) in fragments {
        println!("cargo:rerun-if-changed={path}");
    }
    for name in ["grammar", "type_probe"] {
        let template = format!("src/{name}.lalrpop.in");
        println!("cargo:rerun-if-changed={template}");
        let mut grammar = fs::read_to_string(&template).expect("authored grammar template");
        for (marker, path) in fragments {
            assert_eq!(
                grammar.matches(marker).count(),
                1,
                "exactly one {marker} in {template}"
            );
            grammar = grammar.replace(
                marker,
                &fs::read_to_string(path).expect("shared grammar fragment"),
            );
        }
        let path = out.join(format!("{name}.lalrpop"));
        fs::write(&path, grammar).expect("composed grammar");
        lalrpop::Configuration::new()
            .set_out_dir(&out)
            .process_file(path)
            .expect("Wrela grammar generation failed (including conflicts)");
    }
}
