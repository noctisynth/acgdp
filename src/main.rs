mod app;
mod cli;
mod detect;
mod extract;

fn main() {
    let color = cli::requested_color();
    if let Err(error) = app::run(color) {
        cli::print_error(color, &error);
        std::process::exit(1);
    }
}
