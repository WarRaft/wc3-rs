use super::*;

const DATA: &str = "[TriggerEvents]\nMapInitializationEvent=0,nothing\n[TriggerActions]\nDoNothing=0,nothing\n";
const TEXT: &str = "[wtg]\nversion=7\ngame=2\n\n[trigger]\nname=\"Init\"\ndescription=\"\"\ncategory=0\nevent=MapInitializationEvent()\naction=DoNothing()\n";

#[test]
fn text_and_back_with_the_editors_data() {
    let data = TriggerData::parse(DATA);
    let bytes = convert(TO_BYTES, "war3map.wtg", TEXT.as_bytes(), &data).unwrap();
    assert_eq!(convert(TO_TEXT, "war3map.wtg", &bytes, &data).unwrap(), TEXT.as_bytes());
    let err = convert(TO_TEXT, "m/war3map.wtg", &bytes, &TriggerData::default()).unwrap_err();
    assert!(err.starts_with("Can't read war3map.wtg: no UI\\TriggerData.txt"), "{err}");
    let err = convert(TO_BYTES, "war3map.wtg", TEXT.replace("DoNothing()", "DoNothing(x)").as_bytes(), &data).unwrap_err();
    assert_eq!(err, "Can't save war3map.wtg: line 10: `action`: `DoNothing` takes 0 parameters (UI\\TriggerData.txt), not 1");
}

#[test]
fn diagnostics_only_for_trigger_text() {
    let text = TEXT.replace("action=", "acton=");
    let doc = bni_rs::parse(&text);
    let errors = diagnostics(Some(Path::new("war3map.wtg")), &doc, &text);
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [10]);
    assert!(diagnostics(Some(Path::new("war3map.ini")), &doc, &text).is_empty());
}
