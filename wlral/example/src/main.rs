use std::cell::RefCell;
use std::rc::Rc;
use wlral::compositor::Compositor;
use wlral::geometry::{Displacement, Rectangle};
use wlral::input::event_filter::EventFilter;
use wlral::input::events::*;
use wlral::output_manager::OutputManager;
use wlral::window::Window;
use wlral::window_management_policy::*;
use wlral::window_manager::WindowManager;

enum Gesture {
  Move(MoveEvent),
  Resize(ResizeEvent, Rectangle),
}

struct FloatingWindowManager {
  output_manager: Rc<RefCell<OutputManager>>,
  window_manager: Rc<RefCell<WindowManager>>,

  gesture: Option<Gesture>,
}

impl WindowManagementPolicy for FloatingWindowManager {
  fn handle_window_ready(&mut self, window: Rc<Window>) {
    let output = self
      .output_manager
      .borrow()
      .outputs()
      .iter()
      .find(|output| output.extents().overlaps(&window.extents()))
      .cloned()
      .or_else(|| self.output_manager.borrow().outputs().first().cloned());

    // Center the new window
    if let Some(output) = output {
      window.move_to(
        output.top_left() + ((output.size() - window.extents().size()) / 2.0).as_displacement(),
      );
    }

    // Focus the new window
    self
      .window_manager
      .borrow_mut()
      .focus_window(window.clone());
  }

  fn handle_request_move(&mut self, event: MoveEvent) {
    if !self.window_manager.borrow().window_has_focus(&event.window) {
      // Deny move requests from unfocused clients
      return;
    }

    self.gesture = Some(Gesture::Move(event))
  }
  fn handle_request_resize(&mut self, event: ResizeEvent) {
    if !self.window_manager.borrow().window_has_focus(&event.window) {
      // Deny resize requests from unfocused clients
      return;
    }

    let original_extents = event.window.extents();
    self.gesture = Some(Gesture::Resize(event, original_extents))
  }
}

impl EventFilter for FloatingWindowManager {
  fn handle_pointer_motion_event(&mut self, event: &MotionEvent) -> bool {
    match &self.gesture {
      Some(Gesture::Move(gesture)) => {
        gesture
          .window
          .move_to((event.position() - gesture.drag_point.as_displacement()).into());
        true
      }
      Some(Gesture::Resize(gesture, original_extents)) => {
        let displacement = Displacement::from(event.position() - gesture.cursor_position);
        let mut extents = original_extents.clone();

        if gesture.edges.contains(WindowEdge::TOP) {
          extents.top_left.y += displacement.dy;
          extents.size.height -= displacement.dy;
        } else if gesture.edges.contains(WindowEdge::BOTTOM) {
          extents.size.height += displacement.dy;
        }

        if gesture.edges.contains(WindowEdge::LEFT) {
          extents.top_left.x += displacement.dx;
          extents.size.width -= displacement.dx;
        } else if gesture.edges.contains(WindowEdge::RIGHT) {
          extents.size.width += displacement.dx;
        }

        gesture.window.move_to(extents.top_left);
        gesture.window.resize(extents.size);

        true
      }
      _ => false,
    }
  }

  fn handle_pointer_button_event(&mut self, event: &ButtonEvent) -> bool {
    match (&self.gesture, event.state()) {
      (Some(_), ButtonState::Released) => {
        self.gesture = None;
        true
      }
      _ => false,
    }
  }
}

fn main() {
  let compositor = Compositor::init().expect("Could not initialize compositor");
  let window_manager = FloatingWindowManager {
    output_manager: compositor.output_manager(),
    window_manager: compositor.window_manager(),

    gesture: None,
  };
  compositor.run(window_manager);
}
