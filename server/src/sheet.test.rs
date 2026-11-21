use super::*;

const SHEET: &str = "ID;PWXL;N;E\nB;Y2;X2\nC;Y1;X1;K\"id\"\nC;X2;K\"hp\";AMax\x1B :HP\nC;Y2;X1;K\"hfoo\"\nC;X2;K420;ER[-1]C\nE\n";

#[test]
fn the_sheet_as_json() {
    assert_eq!(
        table(SHEET),
        "{\"rows\": 2, \"cols\": 2, \"errors\": 0, \"cells\": [[1,1,\"s\",\"id\",null,null],[1,2,\"s\",\"hp\",\"Max\\nHP\",null],[2,1,\"s\",\"hfoo\",null,null],[2,2,\"n\",\"420\",null,\"R[-1]C\"]]}"
    );
    // A value that isn't one still shows, as text; the errors are counted.
    assert_eq!(table("ID;P\nC;Kbad\n"), "{\"rows\": 1, \"cols\": 1, \"errors\": 2, \"cells\": [[1,1,\"x\",\"bad\",null,null]]}");
}

#[test]
fn edits_in_editor_coordinates() {
    // Replace a value: line 5 (0-based), after `C;X2;K`.
    assert_eq!(set_cell(SHEET, 2, 2, "500"), "[[5,6,5,9,\"500\"]]");
    // Text stays text; a new cell grows B.
    assert_eq!(set_cell(SHEET, 2, 1, "123"), "[[4,9,4,15,\"\\\"123\\\"\"]]");
    assert_eq!(set_cell(SHEET, 2, 3, "1"), "[[6,0,6,0,\"C;X3;K1\\n\"],[1,6,1,7,\"3\"]]");
    // Clear: the record goes.
    assert_eq!(set_cell(SHEET, 2, 1, ""), "[[4,0,5,0,\"\"],[5,1,5,1,\";Y2\"]]");
    assert_eq!(set_cell(SHEET, 0, 1, "x"), "[]");
}
