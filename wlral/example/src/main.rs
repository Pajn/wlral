use std::cell::RefCell;
use std::rc::Rc;
use wlral::compositor::Compositor;
use wlral::output::OutputManager;
use wlral::surface::Surface;
use wlral::window_management_policy::WindowManagementPolicy;

struct FloatingWindowManager {
  output_manager: Rc<RefCell<OutputManager>>,
}

impl WindowManagementPolicy for FloatingWindowManager {
  fn handle_window_ready(&mut self, surface: Rc<Surface>) {
    let output = self
      .output_manager
      .borrow()
      .outputs()
      .iter()
      .find(|output| output.extents().overlaps(&surface.extents()))
      .cloned()
      .or_else(|| self.output_manager.borrow().outputs().first().cloned());

    if let Some(output) = output {
      surface.move_to(
        output.top_left() + ((output.size() - surface.extents().size()) / 2.0).as_displacement(),
      );
    }
  }
}

fn main() {
  let compositor = Compositor::init().expect("Could not initialize compositor");
  let window_manager = FloatingWindowManager {
    output_manager: compositor.output_manager(),
  };
  compositor.run(window_manager);
}
