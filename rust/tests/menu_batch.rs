use kras_quick::quick_address::{AutoBatchScope, BufferedLineReader};
use std::io::Cursor;

const SCRIPT: &str = "AUTO\n...\n1114014200100080019\n1114014200100080033\n시작\n1\n";

fn drive_batch<R: std::io::Read>(mut reader: BufferedLineReader<R>, expected: AutoBatchScope) {
    assert_eq!(reader.try_read_menu_input().unwrap(), "AUTO");
    assert_eq!(reader.try_read_menu_input().unwrap(), "...");
    let mut queue = Vec::new();
    assert!(kras_quick::batch_input::collect(&mut queue, |suppress| {
        assert!(!suppress);
        let input = reader.try_read_menu_input().unwrap();
        if reader.is_eof() && input.is_empty() { None } else { Some(input) }
    }));
    let scope = kras_quick::batch_input::choose_scope(|| Some(reader.try_read_line().unwrap()));
    assert_eq!(scope, Some(expected));
    assert_eq!(queue, ["1114014200100080019", "1114014200100080033"]);
}

#[test]
fn auto_batch_start_and_scope_remain_separate_commands() {
    drive_batch(BufferedLineReader::new(Cursor::new(SCRIPT.as_bytes()), 8192), AutoBatchScope::LandOnly);
}

#[test]
fn windows_stdin_preserves_auto_batch_commands_and_scope() {
    use std::io::Write;
    use std::process::Stdio;
    if std::env::var_os("KRAS_MENU_STDIN_CHILD").is_some() {
        let expected = match std::env::var("KRAS_MENU_SCOPE").unwrap().as_str() {
            "1" => AutoBatchScope::LandOnly,
            "2" => AutoBatchScope::OrdinaryOnly,
            "3" => AutoBatchScope::BuildingsOnly,
            "4" => AutoBatchScope::FloorsOnly,
            "5" | "" => AutoBatchScope::Full,
            _ => panic!("invalid fixture"),
        };
        drive_batch(BufferedLineReader::new(std::io::stdin(), 8192), expected);
        return;
    }
    for answer in ["1", "2", "3", "4", "5", ""] {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "windows_stdin_preserves_auto_batch_commands_and_scope", "--nocapture"])
            .env("KRAS_MENU_STDIN_CHILD", "1").env("KRAS_MENU_SCOPE", answer)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        let script = format!("AUTO\n...\n1114014200100080019\n1114014200100080033\n시작\n{answer}\n");
        child.stdin.take().unwrap().write_all(script.as_bytes()).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    }
}

#[test]
fn resume_command_does_not_consume_following_scope() {
    let mut reader = BufferedLineReader::new(Cursor::new("재개\n1\n".as_bytes()), 8192);
    assert_eq!(reader.try_read_menu_input().unwrap(), "재개");
    assert_eq!(reader.try_read_line().unwrap(), "1");
}

#[test]
fn batch_input_routes_new_scopes_and_enter_without_eating_next_command() {
    for (answer, expected) in [("3", AutoBatchScope::BuildingsOnly), ("4", AutoBatchScope::FloorsOnly),
        ("5", AutoBatchScope::Full), ("", AutoBatchScope::Full)] {
        let script = format!("AUTO\n...\n1114014200100080019\n시작\n{answer}\nAUTO\n");
        let mut reader = BufferedLineReader::new(Cursor::new(script.as_bytes()), 8192);
        assert_eq!(reader.try_read_menu_input().unwrap(), "AUTO");
        assert_eq!(reader.try_read_menu_input().unwrap(), "...");
        let mut queue = Vec::new();
        assert!(kras_quick::batch_input::collect(&mut queue, |_| Some(reader.try_read_menu_input().unwrap())));
        assert_eq!(kras_quick::batch_input::choose_scope(|| Some(reader.try_read_line().unwrap())), Some(expected));
        assert_eq!(reader.try_read_menu_input().unwrap(), "AUTO");
        assert_eq!(queue, ["1114014200100080019"]);
    }
}
