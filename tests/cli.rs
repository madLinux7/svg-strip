use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
    let directory =
        std::env::temp_dir().join(format!("svg-strip-cli-{name}-{}", std::process::id()));
    fs::create_dir_all(&directory).unwrap();
    let input = directory.join("input.svg");
    let output = directory.join("output.svg");
    fs::write(
        &input,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10" width="10" height="10">
             <style>.st0 { fill-rule: evenodd; }</style>
             <path class="st0" d="M0 0h10v10z"/>
           </svg>"#,
    )
    .unwrap();
    (directory, input, output)
}

#[test]
fn icon_stdout_contains_only_svg_markup() {
    let (directory, input, _) = fixture("stdout");
    let result = Command::new(env!("CARGO_BIN_EXE_svg-strip"))
        .args(["--icon", "20", "-o"])
        .arg(&input)
        .output()
        .unwrap();
    fs::remove_dir_all(directory).unwrap();

    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.starts_with("<svg"));
    assert!(stdout.ends_with("</svg>"));
    assert!(!stdout.contains("Tip:"));
    assert!(!stdout.contains("Stripped SVG written"));
}

#[test]
fn icon_file_summary_ends_with_global_css_tip() {
    let (directory, input, output) = fixture("file");
    let result = Command::new(env!("CARGO_BIN_EXE_svg-strip"))
        .args(["--icon", "20x20"])
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    let written_svg = fs::read_to_string(&output).unwrap();
    fs::remove_dir_all(directory).unwrap();

    assert!(result.status.success());
    assert!(result.stderr.is_empty());
    assert!(written_svg
        .contains(r#"style="width: 20px; height: 20px; overflow: hidden; fill: currentColor""#));

    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.trim_end().ends_with(
        "• Tip: to apply icon color consistently, add this to your global CSS rules:\n\n\
         svg {\n\
         \x20 color: var(--your-icon-color);\n\
         }"
    ));
}
