use iced;

use griphook::app::App;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title("Image Renamer")
        .theme(App::theme)
        .subscription(App::subscription)
        .run()
}
