use std::cell::RefCell;
use std::rc::Rc;
use wlral::compositor::Compositor;
use wlral::geometry::{Displacement, Rectangle};
use wlral::input::event_filter::EventFilter;
use wlral::input::events::*;
use wlral::output::OutputManager;
use wlral::surface::{Surface, SurfaceManager};
use wlral::window_management_policy::*;

enum Gesture {
  Move(MoveEvent),
  Resize(ResizeEvent, Rectangle),
}

struct FloatingWindowManager {
  output_manager: Rc<RefCell<OutputManager>>,
  surface_manager: Rc<RefCell<SurfaceManager>>,

  gesture: Option<Gesture>,
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

  fn handle_request_move(&mut self, event: MoveEvent) {
    if !self
      .surface_manager
      .borrow()
      .surface_has_focus(&event.surface)
    {
      // Deny move requests from unfocused clients
      return;
    }

    self.gesture = Some(Gesture::Move(event))
  }
  fn handle_request_resize(&mut self, event: ResizeEvent) {
    if !self
      .surface_manager
      .borrow()
      .surface_has_focus(&event.surface)
    {
      // Deny resize requests from unfocused clients
      return;
    }

    let original_extents = event.surface.extents();
    self.gesture = Some(Gesture::Resize(event, original_extents))
  }
}

impl EventFilter for FloatingWindowManager {
  fn handle_pointer_motion_event(&mut self, event: &MotionEvent) -> bool {
    match &self.gesture {
      Some(Gesture::Move(gesture)) => {
        gesture
          .surface
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

        gesture.surface.move_to(extents.top_left);
        gesture.surface.resize(extents.size);

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
    surface_manager: compositor.surface_manager(),

    gesture: None,
  };
  compositor.run(window_manager);
}
