use ntui::{element, render};
use rtop::ui::app::App;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), ntui::Error> {
    render(element!(App)).await
}
