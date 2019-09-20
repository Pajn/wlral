use crate::geometry::FPoint;
use crate::input::event_filter::{EventFilter, EventFilterManager};
use crate::input::events::{
  AbsoluteMotionEvent, AxisEvent, ButtonEvent, InputEvent, MotionEvent, RelativeMotionEvent,
};
use crate::input::seat::{Device, InputDeviceManager};
use crate::surface::SurfaceManager;
use std::cell::RefCell;
use std::ffi::CString;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use wlroots_sys::*;

#[allow(unused)]
pub struct CursorManager {
  surface_manager: Rc<RefCell<SurfaceManager>>,
  event_filter_manager: Rc<RefCell<EventFilterManager>>,
  seat: *mut wlr_seat,
  cursor: *mut wlr_cursor,
  cursor_mgr: *mut wlr_xcursor_manager,
  pointers: Vec<Rc<Device>>,

  event_manager: Option<Pin<Box<CursorEventManager>>>,
}

impl CursorManager {
  pub(crate) fn init(
    surface_manager: Rc<RefCell<SurfaceManager>>,
    event_filter_manager: Rc<RefCell<EventFilterManager>>,
    output_layout: *mut wlr_output_layout,
    seat: *mut wlr_seat,
  ) -> Rc<RefCell<CursorManager>> {
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

    let cursor_manager = Rc::new(RefCell::new(CursorManager {
      event_filter_manager,
      surface_manager,
      seat,
      cursor,
      cursor_mgr,
      pointers: vec![],

      event_manager: None,
    }));

    println!("CursorManager::init prebind");

    let mut event_manager = CursorEventManager::new(cursor_manager.clone());
    unsafe {
      event_manager.request_set_cursor(&mut (*seat).events.request_set_cursor);
      event_manager.motion(&mut (*cursor).events.motion);
      event_manager.motion_absolute(&mut (*cursor).events.motion_absolute);
      event_manager.button(&mut (*cursor).events.button);
      event_manager.axis(&mut (*cursor).events.axis);
      event_manager.frame(&mut (*cursor).events.frame);
    }
    cursor_manager.borrow_mut().event_manager = Some(event_manager);

    println!("CursorManager::init postbind");

    cursor_manager
  }

  pub fn has_pointer_device(&self) -> bool {
    !self.pointers.is_empty()
  }

  pub fn position(&self) -> FPoint {
    unsafe {
      FPoint {
        x: (*self.cursor).x,
        y: (*self.cursor).y,
      }
    }
  }

  fn process_motion(&self, event: MotionEvent) {
    let position = self.position();
    let surface = self.surface_manager.borrow().surface_at(&position.into());

    if let Some(surface) = surface {
      let focus_changed =
        unsafe { (*self.seat).pointer_state.focused_surface != surface.surface() };
      let surface_position = position - FPoint::from(surface.buffer_top_left()).as_displacement();

      // "Enter" the surface if necessary. This lets the client know that the
      // cursor has entered one of its surfaces.
      //
      // Note that this gives the surface "pointer focus", which is distinct
      // from cursor focus. You get pointer focus by moving the pointer over
      // a window.
      unsafe {
        wlr_seat_pointer_notify_enter(
          self.seat,
          surface.surface(),
          surface_position.x,
          surface_position.y,
        );
        if !focus_changed {
          // The enter event contains coordinates, so we only need to notify
          // on motion if the focus did not change.
          wlr_seat_pointer_notify_motion(
            self.seat,
            event.time_msec(),
            surface_position.x,
            surface_position.y,
          );
        }
      }
    } else {
      unsafe {
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

    self
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_motion_event(&event);
  }
}

impl InputDeviceManager for CursorManager {
  fn has_any_input_device(&self) -> bool {
    self.has_pointer_device()
  }

  fn add_input_device(&mut self, device: Rc<Device>) {
    // We don't do anything special with pointers. All of our pointer handling
    // is proxied through wlr_cursor. On another compositor, you might take this
    // opportunity to do libinput configuration on the device to set
    // acceleration, etc.
    unsafe {
      wlr_cursor_attach_input_device(self.cursor, device.raw_ptr());
    }

    self.pointers.push(device);
  }

  fn destroy_input_device(&mut self, destroyed_pointer: &Device) {
    self
      .pointers
      .retain(|pointer| pointer.deref() != destroyed_pointer);
  }
}

pub trait CursorEventHandler {
  fn request_set_cursor(&self, event: *const wlr_seat_pointer_request_set_cursor_event);
  fn motion(&self, event: *const wlr_event_pointer_motion);
  fn motion_absolute(&self, event: *const wlr_event_pointer_motion_absolute);
  fn button(&self, event: *const wlr_event_pointer_button);
  fn axis(&self, event: *const wlr_event_pointer_axis);
  fn frame(&self);
}

impl CursorEventHandler for CursorManager {
  fn request_set_cursor(&self, event: *const wlr_seat_pointer_request_set_cursor_event) {
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

  // This event is forwarded by the cursor when a pointer emits a relative
  // pointer motion event (i.e. a delta)

  // The cursor doesn't move unless we tell it to. The cursor automatically
  // handles constraining the motion to the output layout, as well as any
  // special configuration applied for the specific input device which
  // generated the event. You can pass NULL for the device if you want to move
  // the cursor around without any input.
  fn motion(&self, event: *const wlr_event_pointer_motion) {
    let event = unsafe { RelativeMotionEvent::from_ptr(event) };

    let delta = event.delta();
    unsafe {
      wlr_cursor_move(
        self.cursor,
        event.raw_device(),
        delta.delta_x(),
        delta.delta_y(),
      );
      self.process_motion(MotionEvent::Relative(event));
    }
  }

  // This event is forwarded by the cursor when a pointer emits an absolute
  // motion event, from 0..1 on each axis. This happens, for example, when
  // wlroots is running under a Wayland window rather than KMS+DRM, and you
  // move the mouse over the window. You could enter the window from any edge,
  // so we have to warp the mouse there. There is also some hardware which
  // emits these events.
  fn motion_absolute(&self, event: *const wlr_event_pointer_motion_absolute) {
    let event = unsafe { AbsoluteMotionEvent::from_ptr(event) };

    let pos = event.pos();
    unsafe {
      wlr_cursor_warp_absolute(self.cursor, event.raw_device(), pos.x(), pos.y());
      self.process_motion(MotionEvent::Absolute(event));
    }
  }

  fn button(&self, event: *const wlr_event_pointer_button) {
    let event = unsafe { ButtonEvent::from_ptr(event) };

    let handled = self
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_button_event(&event);

    if !handled {
      if event.state() == wlr_button_state_WLR_BUTTON_PRESSED {
        let surface = self
          .surface_manager
          .borrow()
          .surface_at(&self.position().into());

        if let Some(surface) = surface {
          self.surface_manager.borrow_mut().focus_surface(surface);
        }
      }

      unsafe {
        wlr_seat_pointer_notify_button(self.seat, event.time_msec(), event.button(), event.state());
      }
    }
  }

  fn axis(&self, event: *const wlr_event_pointer_axis) {
    let event = unsafe { AxisEvent::from_ptr(event) };

    let handled = self
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_axis_event(&event);

    if !handled {
      unsafe {
        wlr_seat_pointer_notify_axis(
          self.seat,
          event.time_msec(),
          event.orientation(),
          event.delta(),
          event.delta_discrete(),
          event.source(),
        );
      }
    }
  }

  fn frame(&self) {
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
  Rc<RefCell<CursorManager>>,
  [
    request_set_cursor => request_set_cursor_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.borrow().request_set_cursor(data as _)
    };
    motion => motion_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.borrow().motion(data as _)
    };
    motion_absolute => motion_absolute_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.borrow().motion_absolute(data as _)
    };
    button => button_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.borrow().button(data as _)
    };
    axis => axis_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.borrow().axis(data as _)
    };
    frame => frame_func: |this: &mut CursorEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.borrow().frame()
    };
  ]
);

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::seat::SeatEventHandler;
  use crate::test_util::*;
  use std::ptr;
  use std::rc::Rc;

  struct KeyboardManager;

  impl InputDeviceManager for KeyboardManager {
    fn has_any_input_device(&self) -> bool {
      false
    }
    fn add_input_device(&mut self, _: Rc<Device>) {
      // unimplemented!();
    }
    fn destroy_input_device(&mut self, _: &Device) {
      // unimplemented!();
    }
  }

  #[test]
  fn it_drops_and_cleans_up_on_destroy() {
    let surface_manager = Rc::new(RefCell::new(SurfaceManager::init(ptr::null_mut())));
    let event_filter_manager = Rc::new(RefCell::new(EventFilterManager::new()));
    let cursor_manager = Rc::new(RefCell::new(CursorManager {
      surface_manager,
      event_filter_manager,
      seat: ptr::null_mut(),
      cursor: ptr::null_mut(),
      cursor_mgr: ptr::null_mut(),
      pointers: vec![],

      event_manager: None,
    }));

    let mut raw_pointer = wlr_pointer {
      impl_: ptr::null(),
      events: wlr_pointer__bindgen_ty_1 {
        motion: new_wl_signal(),
        motion_absolute: new_wl_signal(),
        button: new_wl_signal(),
        axis: new_wl_signal(),
        frame: new_wl_signal(),
        swipe_begin: new_wl_signal(),
        swipe_update: new_wl_signal(),
        swipe_end: new_wl_signal(),
        pinch_begin: new_wl_signal(),
        pinch_update: new_wl_signal(),
        pinch_end: new_wl_signal(),
      },
      data: ptr::null_mut(),
    };
    let mut device = wlr_input_device {
      impl_: ptr::null(),
      type_: wlr_input_device_type_WLR_INPUT_DEVICE_POINTER,
      vendor: 0,
      product: 0,
      name: ptr::null_mut(),
      width_mm: 0.0,
      height_mm: 0.0,
      output_name: ptr::null_mut(),
      __bindgen_anon_1: wlr_input_device__bindgen_ty_1 {
        pointer: &mut raw_pointer,
      },
      events: wlr_input_device__bindgen_ty_2 {
        destroy: new_wl_signal(),
      },
      data: ptr::null_mut(),
      link: new_wl_list(),
    };

    let destroy_signal = WlSignal::from_ptr(&mut device.events.destroy);

    let seat_event_handler = Rc::new(SeatEventHandler {
      seat: ptr::null_mut(),
      cursor_manager: cursor_manager.clone(),
      keyboard_manager: Rc::new(RefCell::new(KeyboardManager)),
    });
    let device = Device::init(seat_event_handler, &mut device);
    let weak_device = Rc::downgrade(&device);
    cursor_manager.borrow_mut().add_input_device(device);
    let pointer = cursor_manager.borrow().pointers.first().unwrap().clone();

    let weak_pointer = Rc::downgrade(&pointer);
    drop(pointer);

    assert!(weak_device.upgrade().is_some());
    assert!(weak_pointer.upgrade().is_some());
    assert!(destroy_signal.listener_count() == 1);
    assert!(cursor_manager.borrow().has_pointer_device());
    assert!(cursor_manager.borrow().has_any_input_device());

    destroy_signal.emit();

    assert!(weak_device.upgrade().is_none());
    assert!(weak_pointer.upgrade().is_none());
    assert!(destroy_signal.listener_count() == 0);
    assert!(!cursor_manager.borrow().has_pointer_device());
    assert!(!cursor_manager.borrow().has_any_input_device());
  }
}

#[cfg(test)]
unsafe fn wlr_cursor_attach_input_device(_: *mut wlr_cursor, _: *mut wlr_input_device) {}
