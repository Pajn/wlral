use crate::geometry::FPoint;
use crate::surface::SurfaceManager;
use std::cell::RefCell;
use std::ffi::CString;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use wayland_sys::server::signal::wl_signal_add;
use wlroots_sys::*;

pub struct Cursor {
  surface_manager: Rc<RefCell<SurfaceManager>>,
  cursor: *mut wlr_cursor,
  cursor_mgr: *mut wlr_xcursor_manager,
  seat: *mut wlr_seat,
}

impl Cursor {
  fn position(&self) -> FPoint {
    unsafe {
      FPoint {
        x: (*self.cursor).x,
        y: (*self.cursor).y,
      }
    }
  }

  fn process_motion(&mut self, time: u32) {
    // TODO: let window manager handle motion
    unsafe {
      let position = self.position();
      let surface = self.surface_manager.borrow().surface_at(&position.into());

      if let Some(surface) = surface {
        let focus_changed = (*self.seat).pointer_state.focused_surface != surface.surface();
        let surface_position =
          position - FPoint::from(surface.extents().top_left()).as_displacement();
        // "Enter" the surface if necessary. This lets the client know that the
        // cursor has entered one of its surfaces.
        //
        // Note that this gives the surface "pointer focus", which is distinct
        // from keyboard focus. You get pointer focus by moving the pointer over
        // a window.
        wlr_seat_pointer_notify_enter(
          self.seat,
          surface.surface(),
          surface_position.x,
          surface_position.y,
        );
        if !focus_changed {
          // The enter event contains coordinates, so we only need to notify
          // on motion if the focus did not change.
          wlr_seat_pointer_notify_motion(self.seat, time, position.x, position.y);
        }
      } else {
        // If there's no surface under the cursor, set the cursor image to a
        // default. This is what makes the cursor image appear when you move it
        // around the screen, not over any surfaces.
        wlr_xcursor_manager_set_cursor_image(
          self.cursor_mgr,
          CString::new("left_ptr").unwrap().as_ptr(),
          self.cursor,
        );
        wlr_seat_pointer_clear_focus(self.seat);
      }
    }
  }
}

pub trait CursorEventHandler {
  fn request_set_cursor(&mut self, event: *const wlr_seat_pointer_request_set_cursor_event);
  fn motion(&mut self, event: *const wlr_event_pointer_motion);
  fn motion_absolute(&mut self, event: *const wlr_event_pointer_motion_absolute);
  fn button(&mut self, event: *const wlr_event_pointer_button);
  fn axis(&mut self, event: *const wlr_event_pointer_axis);
  fn frame(&mut self);
}

impl CursorEventHandler for Cursor {
  fn request_set_cursor(&mut self, event: *const wlr_seat_pointer_request_set_cursor_event) {
    unsafe {
      // This event is rasied by the seat when a client provides a cursor image
      let focused_client = (*self.seat).pointer_state.focused_client;
      // This can be sent by any client, so we check to make sure this one is
      // actually has pointer focus first.
      if focused_client == (*event).seat_client {
        // Once we've vetted the client, we can tell the cursor to use the
        // provided surface as the cursor image. It will set the hardware cursor
        // on the output that it's currently on and continue to do so as the
        // cursor moves between outputs.
        wlr_cursor_set_surface(
          self.cursor,
          (*event).surface,
          (*event).hotspot_x,
          (*event).hotspot_y,
        );
      }
    }
  }

  fn motion(&mut self, event: *const wlr_event_pointer_motion) {
    // This event is forwarded by the cursor when a pointer emits a relative
    // pointer motion event (i.e. a delta)

    // The cursor doesn't move unless we tell it to. The cursor automatically
    // handles constraining the motion to the output layout, as well as any
    // special configuration applied for the specific input device which
    // generated the event. You can pass NULL for the device if you want to move
    // the cursor around without any input.
    unsafe {
      wlr_cursor_move(
        self.cursor,
        (*event).device,
        (*event).delta_x,
        (*event).delta_y,
      );
      self.process_motion((*event).time_msec);
    }
  }

  fn motion_absolute(&mut self, event: *const wlr_event_pointer_motion_absolute) {
    // This event is forwarded by the cursor when a pointer emits an absolute
    // motion event, from 0..1 on each axis. This happens, for example, when
    // wlroots is running under a Wayland window rather than KMS+DRM, and you
    // move the mouse over the window. You could enter the window from any edge,
    // so we have to warp the mouse there. There is also some hardware which
    // emits these events.
    unsafe {
      wlr_cursor_warp_absolute(self.cursor, (*event).device, (*event).x, (*event).y);
      self.process_motion((*event).time_msec);
    }
  }

  fn button(&mut self, event: *const wlr_event_pointer_button) {
    unsafe {
      wlr_seat_pointer_notify_button(
        self.seat,
        (*event).time_msec,
        (*event).button,
        (*event).state,
      );

      let handled = false;

      if !handled && (*event).state == wlr_button_state_WLR_BUTTON_PRESSED {
        let surface = self
          .surface_manager
          .borrow()
          .surface_at(&self.position().into());

        if let Some(surface) = surface {
          self.surface_manager.borrow_mut().focus_surface(surface);
        }
      }
    }
  }

  fn axis(&mut self, event: *const wlr_event_pointer_axis) {
    // Notify the client with pointer focus of the axis event.
    unsafe {
      wlr_seat_pointer_notify_axis(
        self.seat,
        (*event).time_msec,
        (*event).orientation,
        (*event).delta,
        (*event).delta_discrete,
        (*event).source,
      );
    }
  }

  fn frame(&mut self) {
    // This event is forwarded by the cursor when a pointer emits an frame
    // event. Frame events are sent after regular pointer events to group
    // multiple events together. For instance, two axis events may happen at the
    // same time, in which case a frame event won't be sent in between.
    // Notify the client with pointer focus of the frame event.
    unsafe {
      wlr_seat_pointer_notify_frame(self.seat);
    }
  }
}

wayland_listener!(
  pub CursorEventManager,
  Rc<RefCell<Cursor>>,
  [
     request_set_cursor => request_set_cursor_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().request_set_cursor(data as _)
     };
     motion => motion_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().motion(data as _)
     };
     motion_absolute => motion_absolute_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().motion_absolute(data as _)
     };
     button => button_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().button(data as _)
     };
     axis => axis_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().axis(data as _)
     };
     frame => frame_func: |this: &mut CursorEventManager, _data: *mut libc::c_void,| unsafe {
         let ref mut handler = this.data;
         handler.borrow_mut().frame()
     };
  ]
);

#[allow(unused)]
pub struct CursorManager {
  cursor: *mut wlr_cursor,
  cursor_mgr: *mut wlr_xcursor_manager,
  pointers: Vec<*mut wlr_input_device>,

  event_manager: Pin<Box<CursorEventManager>>,
  event_handler: Rc<RefCell<dyn CursorEventHandler>>,
}

impl CursorManager {
  pub fn init(
    surface_manager: Rc<RefCell<SurfaceManager>>,
    output_layout: *mut wlr_output_layout,
    seat: *mut wlr_seat,
  ) -> CursorManager {
    // Creates a cursor, which is a wlroots utility for tracking the cursor
    // image shown on screen.
    let cursor = unsafe { wlr_cursor_create() };
    unsafe { wlr_cursor_attach_output_layout(cursor, output_layout) };

    // Creates an xcursor manager, another wlroots utility which loads up
    // Xcursor themes to source cursor images from and makes sure that cursor
    // images are available at all scale factors on the screen (necessary for
    // HiDPI support). We add a cursor theme at scale factor 1 to begin with.
    let cursor_mgr = unsafe { wlr_xcursor_manager_create(ptr::null(), 24) };
    unsafe { wlr_xcursor_manager_load(cursor_mgr, 1.0) };

    println!("CursorManager::init prebind");

    let event_handler = Rc::new(RefCell::new(Cursor {
      surface_manager,
      cursor,
      cursor_mgr,
      seat,
    }));

    let mut event_manager = CursorEventManager::new(event_handler.clone());
    unsafe {
      wl_signal_add(
        &mut (*seat).events.request_set_cursor,
        event_manager.request_set_cursor(),
      );
      wl_signal_add(&mut (*cursor).events.motion, event_manager.motion());
      wl_signal_add(
        &mut (*cursor).events.motion_absolute,
        event_manager.motion_absolute(),
      );
      wl_signal_add(&mut (*cursor).events.button, event_manager.button());
      wl_signal_add(&mut (*cursor).events.axis, event_manager.axis());
      wl_signal_add(&mut (*cursor).events.frame, event_manager.frame());
    }

    println!("CursorManager::init postbind");

    CursorManager {
      cursor,
      cursor_mgr,
      pointers: vec![],

      event_manager,
      event_handler,
    }
  }

  pub fn has_pointer_device(&self) -> bool {
    !self.pointers.is_empty()
  }

  pub fn add_pointer_device(&mut self, device: *mut wlr_input_device) {
    // We don't do anything special with pointers. All of our pointer handling
    // is proxied through wlr_cursor. On another compositor, you might take this
    // opportunity to do libinput configuration on the device to set
    // acceleration, etc.
    unsafe {
      wlr_cursor_attach_input_device(self.cursor, device);
    }
    self.pointers.push(device);
  }
}
