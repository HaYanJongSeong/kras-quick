use quick_xml::{events::Event, Reader};
use serde_json::{json, Value};

fn append(stack: &mut [Value], document: &mut Vec<Value>, node: Value) {
    if let Some(parent) = stack.last_mut() {
        parent["children"].as_array_mut().expect("element children").push(node);
    } else { document.push(node); }
}

fn decoded(raw: &str) -> String {
    quick_xml::escape::unescape(raw).map(|value| value.into_owned()).unwrap_or_else(|_| raw.to_owned())
}

pub fn convert(xml: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_str(xml);
    let mut document = Vec::new();
    let mut stack: Vec<Value> = Vec::new();
    let mut roots = 0;
    loop {
        let event = reader.read_event()?;
        match &event {
            Event::Start(start) | Event::Empty(start) => {
                if stack.is_empty() { roots += 1; }
                let name = std::str::from_utf8(start.name().as_ref())?.to_owned();
                let mut attributes = Vec::new();
                for attr in start.attributes() {
                    let attr = attr?;
                    let raw = std::str::from_utf8(attr.value.as_ref())?;
                    attributes.push(json!({"name": std::str::from_utf8(attr.key.as_ref())?, "value": decoded(raw), "raw": raw}));
                }
                let node = json!({"kind":"element", "name":name, "attributes":attributes, "children":[], "selfClosing":matches!(event, Event::Empty(_))});
                if matches!(event, Event::Empty(_)) { append(&mut stack, &mut document, node); }
                else { stack.push(node); }
            }
            Event::End(_) => {
                let node = stack.pop().ok_or("대응하는 시작 태그가 없습니다.")?;
                append(&mut stack, &mut document, node);
            }
            Event::Text(text) => {
                let raw = std::str::from_utf8(text.as_ref())?;
                append(&mut stack, &mut document, json!({"kind":"text", "value":decoded(raw), "raw":raw}));
            }
            Event::GeneralRef(reference) => {
                let raw = format!("&{};", std::str::from_utf8(reference.as_ref())?);
                append(&mut stack, &mut document, json!({"kind":"reference", "value":decoded(&raw), "raw":raw}));
            }
            Event::Eof => break,
            other => {
                let kind = match other {
                    Event::CData(_) => "cdata", Event::Comment(_) => "comment",
                    Event::Decl(_) => "declaration", Event::PI(_) => "instruction",
                    Event::DocType(_) => "doctype", _ => unreachable!(),
                };
                append(&mut stack, &mut document, json!({"kind":kind, "value":std::str::from_utf8(other.as_ref())?}));
            }
        }
    }
    if !stack.is_empty() || roots != 1 { return Err("XML 문서의 루트 또는 닫는 태그가 올바르지 않습니다.".into()); }
    Ok(json!({"schemaVersion":1, "document":document, "summary":crate::kras_output::create_oz_structure(&crate::kras_output::parse_kras_xml(xml))}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "승인된 저장 XML에서 전체 JSON을 생성하며 기존 파일은 덮어쓰지 않음"]
    fn export_saved_xml_without_overwriting() {
        use std::io::Write;
        let root = std::path::PathBuf::from(std::env::var("KRAS_QUICK_ROOT").expect("저장 루트 명시 필요")).join("KRAS");
        let mut directories = vec![root];
        let mut created = 0;
        let mut retained = 0;
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let entry = entry.unwrap();
                let kind = entry.file_type().unwrap();
                if kind.is_dir() { directories.push(entry.path()); continue; }
                if !kind.is_file() || entry.path().extension().and_then(|v| v.to_str()) != Some("xml") { continue; }
                let source = entry.path();
                let xml = std::fs::read_to_string(&source).unwrap();
                let value = convert(&xml).unwrap();
                let destination = source.with_extension("json");
                if destination.exists() {
                    let saved: Value = serde_json::from_slice(&std::fs::read(&destination).unwrap()).expect("기존 JSON 보존: 형식 확인 필요");
                    assert_eq!(saved, value, "기존 JSON은 덮어쓰지 않음: {}", destination.display());
                    retained += 1;
                } else {
                    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&destination).unwrap();
                    file.write_all(&serde_json::to_vec_pretty(&value).unwrap()).unwrap();
                    file.sync_all().unwrap();
                    let saved: Value = serde_json::from_slice(&std::fs::read(&destination).unwrap()).unwrap();
                    assert_eq!(saved, value);
                    created += 1;
                }
                println!("FULL_JSON_VERIFIED={}", destination.display());
            }
        }
        println!("FULL_JSON_CREATED={created} RETAINED={retained}");
    }
    #[test]
    fn preserves_full_structure_attributes_repetition_empty_values_and_strings() {
        let xml = r#"<?xml version="1.0"?><r xmlns:n="urn:test" code="001"><n:a>0007</n:a><n:a/><n:a></n:a><extra x="A&amp;B">한글<![CDATA[<x>]]><!--note--><?test v?>&#48;&amp;</extra></r>"#;
        let value = convert(xml).unwrap();
        let root = &value["document"][1];
        assert_eq!(root["attributes"][1]["value"], "001");
        assert_eq!(root["children"][0]["name"], "n:a");
        assert_eq!(root["children"][0]["children"][0]["value"], "0007");
        assert_eq!(root["children"][1]["selfClosing"], true);
        assert_eq!(root["children"][2]["selfClosing"], false);
        let extra = &root["children"][3];
        assert_eq!(extra["attributes"][0]["value"], "A&B");
        assert_eq!(extra["children"][1]["value"], "<x>");
        assert_eq!(extra["children"][2]["kind"], "comment");
        assert_eq!(extra["children"][3]["kind"], "instruction");
        assert_eq!(extra["children"][4]["value"], "0");
        assert_eq!(extra["children"][5]["value"], "&");
        assert_eq!(root["children"].as_array().unwrap().len(), 4);
    }
    #[test]
    fn rejects_malformed_or_incomplete_xml() {
        for xml in ["<a><b></a>", "<a>", "", "<a/><b/>"] { assert!(convert(xml).is_err(), "{xml}"); }
    }
}
