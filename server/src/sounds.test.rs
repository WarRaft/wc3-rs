use super::*;

fn file() -> Vec<u8> {
    let mut sounds = w3s_rs::Sounds::new();
    sounds.sounds.push(w3s_rs::Sound::new("gg_snd_A", "a.wav"));
    sounds.to_bytes().unwrap()
}

#[test]
fn text_and_back() {
    let name = "/maps/x/war3map.w3s";
    let text = String::from_utf8(convert(TO_TEXT, name, &file()).unwrap()).unwrap();
    assert!(text.starts_with("[w3s]\nversion=1\n\n[gg_snd_A]\nfile=\"a.wav\"\n"), "{text}");
    assert_eq!(convert(TO_BYTES, name, text.as_bytes()).unwrap(), file());
    let err = convert(TO_BYTES, name, b"[w3s]\n[A]\nvolume=x\n").unwrap_err();
    assert_eq!(err, "Can't save war3map.w3s: line 3: `volume`: a whole number, or `default`");
    assert!(convert(TO_TEXT, name, &[1]).unwrap_err().starts_with("Can't read war3map.w3s"));
}

#[test]
fn diagnostics_only_for_sound_text() {
    let doc = bni_rs::parse("[w3s]\n[A]\npitch=x\n");
    let errors = diagnostics(Some(Path::new("war3map.w3s")), &doc);
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].line, 3);
    assert!(diagnostics(Some(Path::new("x.ini")), &doc).is_empty());
    assert!(diagnostics(Some(Path::new("war3map.w3s")), &bni_rs::parse("[a]\n")).is_empty());
}
