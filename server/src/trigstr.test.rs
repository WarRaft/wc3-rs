use super::*;
use crate::document::Language;
use crate::project::tests::TempDir;

#[test]
fn references_under_the_cursor() {
    let line = r#"call SetName(u, "TRIGSTR_043") // TRIGSTR_ TRIGSTR_7"#;
    let at = |col| reference_at(line, col);
    assert_eq!(at(17), Some(("TRIGSTR_043".into(), 43)));
    assert_eq!(at(28), Some(("TRIGSTR_043".into(), 43)));
    assert_eq!(at(16), None);
    // No digits: not one.
    assert_eq!(at(35), None);
    assert_eq!(at(50), Some(("TRIGSTR_7".into(), 7)));
}

#[test]
fn colors_and_line_breaks() {
    assert_eq!(
        colored("A |cffffcc00holy|r *warrior*|nof <light>\nend|cff00ff00"),
        "A <span style=\"color:#ffcc00;\">holy</span> \\*warrior\\*<br>of &lt;light&gt;<br>end<span style=\"color:#00ff00;\"></span>"
    );
    // A stray `|r` closes nothing.
    assert_eq!(colored("a|rb|x"), "ab\\|x");
}

#[test]
fn hover_from_the_table_next_to_the_file() {
    let tmp = TempDir::new("trigstr-hover");
    std::fs::write(
        tmp.0.join("war3map.wts"),
        "\u{FEFF}STRING 43\r\n// Units: h000 (Knight), Name (unam)\r\n{\r\n|cffffcc00Knight|r\r\n}\r\n",
    )
    .unwrap();
    let mut ws = Workspace::default();
    let j = tmp.0.join("war3map.j");
    let text = "call SetName(u, \"TRIGSTR_043\")\ncall SetName(u, \"TRIGSTR_9\")\n";
    ws.open(path_id(&j), Some(j.clone()), Language::Jass, 1, text.into());
    let found = hover(&ws, &path_id(&j), Position::new(0, 20)).unwrap();
    assert_eq!(
        found,
        "**TRIGSTR_043** · `war3map.wts`, line 1\n\n<span style=\"color:#ffcc00;\">Knight</span>\n\n---\n\n*// Units: h000 (Knight), Name (unam)*"
    );
    assert_eq!(hover(&ws, &path_id(&j), Position::new(1, 20)).unwrap(), "**TRIGSTR_9**\n\nNot in `war3map.wts`.");
    // Off a reference.
    assert_eq!(hover(&ws, &path_id(&j), Position::new(0, 2)), None);
    // The editor's copy of the table wins over the disk's.
    let wts = tmp.0.join("war3map.wts");
    ws.open(path_id(&wts), Some(wts.clone()), Language::Wts, 1, "STRING 43\n{\nPaladin\n}\n".into());
    assert!(hover(&ws, &path_id(&j), Position::new(0, 20)).unwrap().contains("\n\nPaladin"));
}
