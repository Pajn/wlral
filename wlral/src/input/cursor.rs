use crate::geometry::FPoint;
use crate::input::event_filter::{EventFilter, EventFilterManager};
use crate::input::events::*;
use crate::input::seat::{Device, InputDeviceManager};
use crate::{
  output_manager::{OutputEventListener, OutputManager},
  window_manager::WindowManager,
};
use log::debug;
use std::cell::RefCell;
use std::ffi::CString;
use std::ops::Deref;
use std::pin::Pin;
use std::ptr;
use std::rc::Rc;
use wlroots_sys::*;

#[cfg(test)]
use mockall::*;

#[cfg_attr(test, automock)]
pub trait CursorManager {
  /// If there are any pointer device (mouse, touchpad, etc.) attached
  fn has_pointer_device(&self) -> bool;

  /// Get the position of the cursor in global coordinates
  fn position(&self) -> FPoint;

  fn raw_cursor(&self) -> *mut wlr_cursor;
}

pub struct CursorManagerImpl {
  output_manager: Rc<dyn OutputManager>,
  window_manager: Rc<RefCell<WindowManager>>,
  event_filter_manager: Rc<RefCell<EventFilterManager>>,
  seat: *mut wlr_seat,
  cursor: *mut wlr_cursor,
  cursor_mgr: *mut wlr_xcursor_manager,
  pointers: Vec<Rc<Device>>,

  event_manager: Option<Pin<Box<CursorEventManager>>>,
}

impl CursorManager for CursorManagerImpl {
  fn has_pointer_device(&self) -> bool {
    !self.pointers.is_empty()
  }

  fn position(&self) -> FPoint {
    unsafe {
      FPoint {
        x: (*self.cursor).x,
        y: (*self.cursor).y,
      }
    }
  }

  fn raw_cursor(&self) -> *mut wlr_cursor {
    self.cursor
  }
}

impl CursorManagerImpl {
  pub(crate) fn init(
    output_manager: Rc<dyn OutputManager>,
    window_manager: Rc<RefCell<WindowManager>>,
    event_filter_manager: Rc<RefCell<EventFilterManager>>,
    output_layout: *mut wlr_output_layout,
    seat: *mut wlr_seat,
  ) -> Rc<RefCell<CursorManagerImpl>> {
    let cursor = unsafe { wlr_cursor_create() };
    unsafe {
      wlr_cursor_attach_output_layout(cursor, output_layout);
    };

    // Creates an xcursor manager, another wlroots utility which loads up
    // Xcursor themes to source cursor images from and makes sure that cursor
    // images are available at all scale factors on the screen (necessary for
    // HiDPI support). We add a cursor theme at scale factor 1 to begin with.
    let cursor_mgr = unsafe { wlr_xcursor_manager_create(ptr::null(), 24) };
    unsafe { wlr_xcursor_manager_load(cursor_mgr, 1.0) };

    let cursor_manager = Rc::new(RefCell::new(CursorManagerImpl {
      output_manager: output_manager.clone(),
      window_manager,
      event_filter_manager,
      seat,
      cursor,
      cursor_mgr,
      pointers: vec![],

      event_manager: None,
    }));

    output_manager.subscribe(cursor_manager.clone());

    debug!("CursorManager::init");

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

    cursor_manager
  }

  fn refresh_device_mappings(&self) {
    debug!("CursorManager::refresh_device_mappings");
    for pointer in self.pointers.iter() {
      if let Some(output_name) = pointer.output_name() {
        for output in self.output_manager.outputs().borrow().iter() {
          if output_name == output.name() {
            unsafe {
              wlr_cursor_map_input_to_output(self.cursor, pointer.raw_ptr(), output.raw_ptr());
            }
          }
        }
      }
    }
  }

  fn process_motion(&self, event: MotionEvent) {
    let position = event.position();

    unsafe {
      wlr_cursor_warp(self.cursor, event.raw_device(), position.x(), position.y());
    }

    let surface = self
      .window_manager
      .borrow()
      .window_buffer_at(&position.into());

    if let Some(surface) = surface {
      let focus_changed =
        unsafe { (*self.seat).pointer_state.focused_surface != surface.wlr_surface() };
      let surface_position =
        position - FPoint::from(surface.buffer_extents().top_left()).as_displacement();

      // "Enter" the surface if necessary. This lets the client know that the
      // cursor has entered one of its surfaces.
      //
      // Note that this gives the surface "pointer focus", which is distinct
      // from cursor focus. You get pointer focus by moving the pointer over
      // a window.
      unsafe {
        wlr_seat_pointer_notify_enter(
          self.seat,
          surface.wlr_surface(),
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

impl OutputEventListener for RefCell<CursorManagerImpl> {
  fn new_output(&self, _output: &crate::output::Output) {
    self.borrow().refresh_device_mappings()
  }
  fn destroyed_output(&self, _output: &crate::output::Output) {
    self.borrow().refresh_device_mappings()
  }
}

impl InputDeviceManager for CursorManagerImpl {
  fn has_any_input_device(&self) -> bool {
    self.has_pointer_device()
  }

  fn add_input_device(&mut self, device: Rc<Device>) {
    debug!("CursorManager::add_input_device");

    unsafe {
      wlr_cursor_attach_input_device(self.cursor, device.raw_ptr());
    }

    self.pointers.push(device);

    self.refresh_device_mappings();
  }

  fn destroy_input_device(&mut self, destroyed_pointer: &Device) {
    debug!("CursorManager::destroy_input_device");
    self
      .pointers
      .retain(|pointer| pointer.deref() != destroyed_pointer);
  }
}

pub trait CursorEventHandler {
  fn request_set_cursor(&self, event: *const wlr_seat_pointer_request_set_cursor_event);
  fn axis(&self, event: *const wlr_event_pointer_axis);
  fn button(&self, event: *const wlr_event_pointer_button);
  fn motion(&self, event: *const wlr_event_pointer_motion);
  fn motion_absolute(&self, event: *const wlr_event_pointer_motion_absolute);
  fn frame(&self);
}

impl CursorEventHandler for Rc<RefCell<CursorManagerImpl>> {
  fn request_set_cursor(&self, event: *const wlr_seat_pointer_request_set_cursor_event) {
    let manager = self.borrow();
    unsafe {
      // This event is rasied by the seat when a client provides a cursor image
      let focused_client = (*manager.seat).pointer_state.focused_client;
      // This can be sent by any client, so we check to make sure this one is
      // actually has pointer focus first.
      if focused_client == (*event).seat_client {
        // Once we've vetted the client, we can tell the cursor to use the
        // provided surface as the cursor image. It will set the hardware cursor
        // on the output that it's currently on and continue to do so as the
        // cursor moves between outputs.
        wlr_cursor_set_surface(
          manager.cursor,
          (*event).surface,
          (*event).hotspot_x,
          (*event).hotspot_y,
        );
      }
    }
  }

  fn axis(&self, event: *const wlr_event_pointer_axis) {
    let event = unsafe { AxisEvent::from_ptr(self.clone(), event) };

    let handled = self
      .borrow()
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_axis_event(&event);

    if !handled {
      unsafe {
        wlr_seat_pointer_notify_axis(
          self.borrow().seat,
          event.time_msec(),
          event.orientation(),
          event.delta(),
          event.delta_discrete(),
          event.source(),
        );
      }
    }
  }

  fn button(&self, event: *const wlr_event_pointer_button) {
    let event = unsafe { ButtonEvent::from_ptr(self.clone(), event) };

    let handled = self
      .borrow()
      .event_filter_manager
      .borrow_mut()
      .handle_pointer_button_event(&event);

    if !handled {
      if event.state() == ButtonState::Pressed {
        let surface = self
          .borrow()
          .window_manager
          .borrow()
          .window_buffer_at(&self.borrow().position().into());

        if let Some(surface) = surface {
          if surface.can_receive_focus() {
            self
              .borrow()
              .window_manager
              .borrow_mut()
              .focus_window(surface);
          }
        }
      }

      unsafe {
        wlr_seat_pointer_notify_button(
          self.borrow().seat,
          event.time_msec(),
          event.button(),
          event.state().as_raw(),
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
    let event = unsafe { RelativeMotionEvent::from_ptr(self.clone(), event) };

    self.borrow().process_motion(MotionEvent::Relative(event));
  }

  // This event is forwarded by the cursor when a pointer emits an absolute
  // motion event, from 0..1 on each axis. This happens, for example, when
  // wlroots is running under a Wayland window rather than KMS+DRM, and you
  // move the mouse over the window. You could enter the window from any edge,
  // so we have to warp the mouse there. There is also some hardware which
  // emits these events.
  fn motion_absolute(&self, event: *const wlr_event_pointer_motion_absolute) {
    let event = unsafe { AbsoluteMotionEvent::from_ptr(self.clone(), event) };

    self.borrow().process_motion(MotionEvent::Absolute(event));
  }

  fn frame(&self) {
    // This event is forwarded by the cursor when a pointer emits an frame
    // event. Frame events are sent after regular pointer events to group
    // multiple events together. For instance, two axis events may happen at the
    // same time, in which case a frame event won't be sent in between.
    // Notify the client with pointer focus of the frame event.
    unsafe {
      wlr_seat_pointer_notify_frame(self.borrow().seat);
    }
  }
}

wayland_listener!(
  pub CursorEventManager,
  Rc<RefCell<CursorManagerImpl>>,
  [
    request_set_cursor => request_set_cursor_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.request_set_cursor(data as _)
    };
    motion => motion_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.motion(data as _)
    };
    motion_absolute => motion_absolute_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.motion_absolute(data as _)
    };
    button => button_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.button(data as _)
    };
    axis => axis_func: |this: &mut CursorEventManager, data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.axis(data as _)
    };
    frame => frame_func: |this: &mut CursorEventManager, _data: *mut libc::c_void,| unsafe {
      let ref mut handler = this.data;
      handler.frame()
    };
  ]
);

#[cfg(test)]
mod tests {
  use super::*;
  use crate::input::seat::SeatEventHandler;
  use crate::output_manager::MockOutputManager;
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
    let output_manager = Rc::new(MockOutputManager::default());
    let window_manager = Rc::new(RefCell::new(WindowManager::init(ptr::null_mut())));
    let event_filter_manager = Rc::new(RefCell::new(EventFilterManager::new()));
    let cursor_manager = Rc::new(RefCell::new(CursorManagerImpl {
      output_manager,
      window_manager,
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
#[cfg(test)]
unsafe fn wlr_cursor_map_input_to_output(
  _: *mut wlr_cursor,
  _: *mut wlr_input_device,
  _: *mut wlr_output,
) {
}
