use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use wlral::compositor::Compositor;
use wlral::geometry::{Displacement, Rectangle};
use wlral::input::event_filter::EventFilter;
use wlral::input::events::*;
use wlral::output::Output;
use wlral::output_manager::OutputManager;
use wlral::window::Window;
use wlral::window_management_policy::*;
use wlral::window_manager::WindowManager;
use xkbcommon::xkb;

enum Gesture {
  Move(MoveRequest),
  Resize(ResizeRequest, Rectangle),
}

struct FloatingWindowManager {
  output_manager: Rc<RefCell<dyn OutputManager>>,
  window_manager: Rc<RefCell<WindowManager>>,

  gesture: Option<Gesture>,
  restore_size: BTreeMap<usize, Rectangle>,
}

impl FloatingWindowManager {
  fn output_for_window(&self, window: &Window) -> Option<Rc<Output>> {
    self
      .output_manager
      .borrow()
      .outputs()
      .iter()
      .find(|output| output.extents().overlaps(&window.extents()))
      .cloned()
      .or_else(|| self.output_manager.borrow().outputs().first().cloned())
  }
}

impl WindowManagementPolicy for FloatingWindowManager {
  fn handle_window_ready(&mut self, window: Rc<Window>) {
    let output = self.output_for_window(&window);

    if window.can_receive_focus() {
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
  }

  fn handle_request_move(&mut self, request: MoveRequest) {
    if !self
      .window_manager
      .borrow()
      .window_has_focus(&request.window)
    {
      // Deny move requests from unfocused clients
      return;
    }

    if request.window.maximized() {
      request.window.set_maximized(false);
    }
    if request.window.fullscreen() {
      request.window.set_fullscreen(false);
    }

    self.gesture = Some(Gesture::Move(request))
  }
  fn handle_request_resize(&mut self, request: ResizeRequest) {
    if !self
      .window_manager
      .borrow()
      .window_has_focus(&request.window)
    {
      // Deny resize requests from unfocused clients
      return;
    }

    if !request.window.resizing() {
      request.window.set_resizing(true);
    }

    let original_extents = request.window.extents();
    self.gesture = Some(Gesture::Resize(request, original_extents))
  }
  fn handle_request_maximize(&mut self, request: MaximizeRequest) {
    let output = self.output_for_window(&request.window);

    if let Some(output) = output {
      if request.maximize {
        self.restore_size.insert(
          request.window.wlr_surface() as usize,
          request.window.extents(),
        );
        request.window.set_maximized(true);
        request.window.move_to(output.top_left());
        request.window.resize(output.size());
      } else {
        request.window.set_maximized(false);
        if let Some(extents) = self
          .restore_size
          .get(&(request.window.wlr_surface() as usize))
        {
          request.window.move_to(extents.top_left());
          request.window.resize(extents.size());
        }
      }
    }
  }
  fn handle_request_fullscreen(&mut self, request: FullscreenRequest) {
    let output = request
      .output
      .clone()
      .or_else(|| self.output_for_window(&request.window));

    if let Some(output) = output {
      if request.fullscreen {
        self.restore_size.insert(
          request.window.wlr_surface() as usize,
          request.window.extents(),
        );
        request.window.set_fullscreen(true);
        request.window.move_to(output.top_left());
        request.window.resize(output.size());
      } else {
        request.window.set_fullscreen(false);
        if let Some(extents) = self
          .restore_size
          .get(&(request.window.wlr_surface() as usize))
        {
          request.window.move_to(extents.top_left());
          request.window.resize(extents.size());
        }
      }
    }
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
      (Some(gesture), ButtonState::Released) => {
        if let Gesture::Resize(request, _) = gesture {
          if request.window.resizing() {
            request.window.set_resizing(false);
          }
        }

        self.gesture = None;
        true
      }
      _ => false,
    }
  }

  fn handle_keyboard_event(&mut self, event: &KeyboardEvent) -> bool {
    let keysym = event.get_one_sym();

    if keysym == xkb::KEY_Escape
      && event
        .xkb_state()
        .mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_DEPRESSED)
    {
      if let Some(window) = self.window_manager.borrow().focused_window() {
        window.ask_client_to_close();
      }
      true
    } else {
      false
    }
  }
}

fn main() {
  let compositor = Compositor::init().expect("Could not initialize compositor");
  let window_manager = FloatingWindowManager {
    output_manager: compositor.output_manager(),
    window_manager: compositor.window_manager(),

    gesture: None,
    restore_size: BTreeMap::new(),
  };
  compositor.run(window_manager);
}
