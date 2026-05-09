use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let result = if args.is_empty() {
        // No args: open empty scratch buffer
        soe::edit("untitled", "", soe::EditorMode::PlainText).map(|content| {
            if let Some(text) = content {
                print!("{text}");
            }
        })
    } else {
        // Edit each file
        let mut res = Ok(());
        for arg in &args {
            res = soe::edit_file(&PathBuf::from(arg));
            if res.is_err() {
                break;
            }
        }
        res
    };

    if let Err(e) = result {
        eprintln!("soe: {e:#}");
        process::exit(1);
    }
}
