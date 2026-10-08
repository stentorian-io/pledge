use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use interlint::types::{parser::parse_type_string, shape::TypeShape};

// Run hostile inputs in a child so a regression fails instead of hanging the suite.
#[test]
fn complex_types_terminate_with_bounded_work() {
    const CHILD: &str = "PLEDGE_PARSER_LIMIT_CHILD";
    if std::env::var_os(CHILD).is_some() {
        assert!(matches!(
            parse_type_string("[[[[string]]]]"),
            TypeShape::Tuple(_)
        ));
        for input in [
            "[".repeat(1000) + "string" + &"]".repeat(1000),
            "(".repeat(1000) + "string" + &")".repeat(1000),
            "Array<".repeat(1000) + "string" + &">".repeat(1000),
            "string".to_owned() + &"[]".repeat(10000),
            "[".to_owned() + &"string,".repeat(10000) + "]",
        ] {
            assert!(matches!(parse_type_string(&input), TypeShape::Opaque(_)));
        }
        return;
    }

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "complex_types_terminate_with_bounded_work",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "parser child failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("parser exceeded five seconds on bounded input");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
