mod analytics_ui;
mod app;
mod charts;
mod instance;
mod study_ui;
mod ui;
mod worker;

use app::{App, Screen};
use std::path::PathBuf;

fn arguments(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<(Option<PathBuf>, Screen)>, String> {
    let mut args = args.into_iter();
    let mut path = None;
    let mut screen = Screen::Dashboard;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => {
                println!("Ebbi — local productivity\n\nUsage: ebbi [--db PATH] [--view=dashboard|study|analytics|settings]\n\n--db PATH     Use a separate Ebbi database (useful for testing)\n--view=VIEW   Open dashboard, study, analytics or settings\n--version     Show version\n--help        Show help");
                return Ok(None);
            }
            "--version" | "-V" => {
                println!("ebbi {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "--db" => path = Some(PathBuf::from(args.next().ok_or("--db requires a path")?)),
            _ if arg.starts_with("--view=") => {
                screen = match &arg[7..] {
                    "dashboard" => Screen::Dashboard,
                    "settings" => Screen::Settings,
                    "study" => Screen::Study,
                    "analytics" => Screen::Analytics,
                    other => {
                        return Err(format!(
                    "View '{other}' is unavailable in this release. Choose dashboard, study, analytics or settings."
                ))
                    }
                }
            }
            _ => return Err(format!("Unknown argument: {arg}. Run ebbi --help.")),
        }
    }
    Ok(Some((path, screen)))
}

fn main() -> iced::Result {
    let parsed = match arguments(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => return Ok(()),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let path = parsed
        .0
        .map(Ok)
        .unwrap_or_else(ebbi::persistence::default_db_path)
        .and_then(|path| instance::database_path(&path));
    let instance = match &path {
        Ok(path) => match instance::Instance::start(path, parsed.1) {
            Ok(Some(instance)) => Some(instance),
            Ok(None) => return Ok(()),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        },
        Err(_) => None,
    };
    iced::application("Ebbi", App::update, ui::view)
        .subscription(App::subscription)
        .theme(|_| ui::theme())
        .window(iced::window::Settings {
            size: iced::Size::new(1100.0, 700.0),
            min_size: Some(iced::Size::new(640.0, 480.0)),
            platform_specific: iced::window::settings::PlatformSpecific {
                application_id: "ebbi".into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .run_with(move || {
            let (mut app, task) = App::new(path, parsed.1);
            app.instance = instance;
            (app, task)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cli_only_accepts_available_views_and_requires_a_path() {
        assert!(matches!(
            arguments(["--view=study".into()]),
            Ok(Some((None, Screen::Study)))
        ));
        assert!(arguments(["--view=missing".into()]).is_err());
        assert!(arguments(["--db".into()]).is_err());
        assert!(matches!(
            arguments(["--view=settings".into()]),
            Ok(Some((None, Screen::Settings)))
        ));
    }
}
