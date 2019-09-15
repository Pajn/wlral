use wlral::compositor::Compositor;

fn main() {
    let compositor = Compositor::init().expect("Could not initialize compositor");
    compositor.run();
}
