use std::collections::BTreeMap;
use std::rc::Rc;
use wlral::compositor::Compositor;
use wlral::geometry::{Displacement, Rectangle};
use wlral::input::event_filter::EventFilter;
use wlral::input::events::*;
use wlral::output::Output;
use wlral::output_management_protocol::OutputManagementProtocol;
use wlral::output_manager::OutputManager;
use wlral::window::{Window, WindowEdge};
use wlral::window_management_policy::*;
use wlral::window_manager::WindowManager;
use xkbcommon::xkb;

enum Gesture {
  Move(MoveRequest),
  Resize(ResizeRequest, Rectangle),
}

struct FloatingWindowManager {
  output_manager: Rc<OutputManager>,
  window_manager: Rc<WindowManager>,
  output_management_protocol: Rc<OutputManagementProtocol>,

  gesture: Option<Gesture>,
  restore_size: BTreeMap<usize, Rectangle>,
}

impl FloatingWindowManager {
  fn output_for_window(&self, window: &Window) -> Option<Rc<Output>> {
    self
      .output_manager
      .outputs()
      .iter()
      .find(|output| output.extents().overlaps(&window.extents()))
      .cloned()
      .or_else(|| self.output_manager.outputs().first().cloned())
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
      self.window_manager.focus_window(window.clone());
    }
  }

  fn handle_request_activate(&mut self, request: ActivateRequest) {
    self.window_manager.focus_window(request.window);
  }

  fn handle_request_close(&mut self, request: CloseRequest) {
    request.window.ask_client_to_close();
  }

  fn handle_request_move(&mut self, request: MoveRequest) {
    if !self.window_manager.window_has_focus(&request.window) {
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
    if !self.window_manager.window_has_focus(&request.window) {
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
        request.window.set_extents(&Rectangle {
          top_left: output.top_left(),
          size: output.size(),
        });
      } else {
        request.window.set_maximized(false);
        if let Some(extents) = self
          .restore_size
          .get(&(request.window.wlr_surface() as usize))
        {
          request.window.set_extents(extents);
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
        request.window.set_extents(&Rectangle {
          top_left: output.top_left(),
          size: output.size(),
        });
      } else {
        request.window.set_fullscreen(false);
        if let Some(extents) = self
          .restore_size
          .get(&(request.window.wlr_surface() as usize))
        {
          request.window.set_extents(extents);
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

        gesture.window.set_extents(&extents);

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

    if event.state() != KeyState::Pressed {
      return false;
    }

    if keysym == xkb::KEY_Escape
      && event
        .xkb_state()
        .mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_DEPRESSED)
    {
      if let Some(window) = self.window_manager.focused_window() {
        window.ask_client_to_close();
      }
      true
    } else if keysym == xkb::KEY_a
      && event
        .xkb_state()
        .mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_DEPRESSED)
      && self.output_management_protocol.has_pending_test()
    {
      self
        .output_management_protocol
        .apply_pending_test()
        .expect("Could not apply pending test");
      true
    } else if keysym == xkb::KEY_c
      && event
        .xkb_state()
        .mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_DEPRESSED)
      && self.output_management_protocol.has_pending_test()
    {
      self
        .output_management_protocol
        .cancel_pending_test()
        .expect("Could not cancel pending test");
      true
    } else if keysym == xkb::KEY_d
      && event
        .xkb_state()
        .mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_DEPRESSED)
      && event
        .xkb_state()
        .mod_name_is_active(xkb::MOD_NAME_ALT, xkb::STATE_MODS_DEPRESSED)
    {
      println!("Windows:");
      for window in self.window_manager.windows() {
        println!("  {}:", window.title().unwrap_or("[no title]".to_string()));
        println!(
          "    app_id: {}",
          window.app_id().unwrap_or("[no app_id]".to_string())
        );
        println!(
          "    outputs: {}",
          window
            .outputs()
            .iter()
            .map(|o| o.name())
            .collect::<Vec<_>>()
            .join(", ")
        );
      }
      true
    } else {
      false
    }
  }
}

fn main() {
  env_logger::init();

  let compositor = Compositor::init();
  compositor.config_manager().update_config(|config| {
    config.background_color = [0.3, 0.3, 0.3];
  });
  let output_management_protocol = compositor
    .enable_output_management_protocol(30_000)
    .unwrap();
  let window_manager = FloatingWindowManager {
    output_manager: compositor.output_manager(),
    output_management_protocol,
    window_manager: compositor.window_manager(),

    gesture: None,
    restore_size: BTreeMap::new(),
  };
  compositor
    .run(window_manager)
    .expect("Could not run compositor");
}
