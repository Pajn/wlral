use crate::geometry::{Displacement, Point, Rectangle, Size, TransformMatrix};
use crate::window::Window;
use crate::window_management_policy::WmPolicyManager;
use crate::{
  event::{Event, EventOnce},
  window_manager::WindowManager,
};
use std::cell::RefCell;
use std::pin::Pin;
use std::ptr;
use std::rc::{Rc, Weak};
use std::{borrow::Cow, ffi::CStr, fmt::Debug, time::Instant};
use wlroots_sys::*;

#[derive(Debug)]
pub struct Output {
  pub(crate) wm_policy_manager: Rc<WmPolicyManager>,
  pub(crate) window_manager: Rc<WindowManager>,

  pub(crate) renderer: *mut wlr_renderer,
  pub(crate) output_layout: *mut wlr_output_layout,
  pub(crate) output: *mut wlr_output,
  pub(crate) created_at: Instant,
  pub(crate) background_color: RefCell<[f32; 3]>,

  pub(crate) on_destroy: EventOnce<()>,
  pub(crate) on_frame: Event<()>,

  pub(crate) event_manager: RefCell<Option<Pin<Box<OutputEventManager>>>>,
}

impl Output {
  pub fn raw_ptr(&self) -> *mut wlr_output {
    self.output
  }

  pub fn use_preferred_mode(&self) -> Result<(), ()> {
    unsafe {
      // Some backends don't have modes. DRM+KMS does, and we need to set a mode
      // before we can use the output. The mode is a tuple of (width, height,
      // refresh rate), and each monitor supports only a specific set of modes. We
      // just pick the first, a more sophisticated compositor would let the user
      // configure it or pick the mode the display advertises as preferred.
      let mode = wlr_output_preferred_mode(self.output);
      if !mode.is_null() {
        wlr_output_set_mode(self.output, mode);
      }

      wlr_output_enable(self.output, true);
      if !wlr_output_commit(self.output) {
        return Err(());
      }
    }
    Ok(())
  }

  /// Sets a custom mode on the output. If modes are available, they are preferred.
  /// Setting `refresh` to zero lets the backend pick a preferred value.
  pub fn set_custom_mode(&self, size: Size, refresh: i32) -> Result<(), ()> {
    unsafe {
      wlr_output_set_custom_mode(self.output, size.width(), size.height(), refresh);

      wlr_output_enable(self.output, true);
      if !wlr_output_commit(self.output) {
        return Err(());
      }
    }
    Ok(())
  }

  pub fn top_left(&self) -> Point {
    let mut x = 0.0;
    let mut y = 0.0;
    unsafe {
      wlr_output_layout_output_coords(self.output_layout, self.output, &mut x, &mut y);
    }
    Point {
      x: -x as i32,
      y: -y as i32,
    }
  }

  pub fn size(&self) -> Size {
    unsafe {
      Size {
        width: (*self.output).width,
        height: (*self.output).height,
      }
    }
  }

  pub fn extents(&self) -> Rectangle {
    Rectangle {
      top_left: self.top_left(),
      size: self.size(),
    }
  }

  pub fn scale(&self) -> f32 {
    unsafe { (*self.output).scale }
  }

  pub fn transform_matrix(&self) -> TransformMatrix {
    unsafe { TransformMatrix((*self.output).transform_matrix) }
  }

  pub fn name(&self) -> Cow<str> {
    let name: &CStr = unsafe { CStr::from_ptr((*self.output).name.as_ptr()) };
    name.to_string_lossy()
  }

  pub fn description(&self) -> Cow<str> {
    let description: &CStr = unsafe { CStr::from_ptr((*self.output).description) };
    description.to_string_lossy()
  }

  pub fn make(&self) -> Cow<str> {
    let make: &CStr = unsafe { CStr::from_ptr((*self.output).make.as_ptr()) };
    make.to_string_lossy()
  }

  pub fn model(&self) -> Cow<str> {
    let model: &CStr = unsafe { CStr::from_ptr((*self.output).model.as_ptr()) };
    model.to_string_lossy()
  }

  pub fn on_destroy(&self) -> &EventOnce<()> {
    &self.on_destroy
  }
  pub fn on_frame(&self) -> &Event<()> {
    &self.on_frame
  }

  pub(crate) fn render_window(&self, frame_time: &timespec, window: Rc<Window>) {
    unsafe {
      let wlr_surface = &mut *window.wlr_surface();

      // We first obtain a wlr_texture, which is a GPU resource. wlroots
      // automatically handles negotiating these with the client. The underlying
      // resource could be an opaque handle passed from the client, or the client
      // could have sent a pixel buffer which we copied to the GPU, or a few other
      // means. You don't have to worry about this, wlroots takes care of it.
      let texture = wlr_surface_get_texture(wlr_surface);
      if texture.is_null() {
        return;
      }

      // The view has a position in layout coordinates. If you have two displays,
      // one next to the other, both 1080p, a view on the rightmost display might
      // have layout coordinates of 2000,100. We need to translate that to
      // output-local coordinates, or (2000 - 1920).
      let buffer_extents = window.buffer_extents();

      let top_left = buffer_extents.top_left() - self.top_left().as_displacement()
        + Displacement {
          dx: wlr_surface.sx,
          dy: wlr_surface.sy,
        }
        + window.translate.borrow().clone();

      // We also have to apply the scale factor for HiDPI outputs. This is only
      // part of the puzzle, TinyWL does not fully support HiDPI.
      let render_box = Rectangle {
        top_left: top_left * self.scale(),
        size: buffer_extents.size() * self.scale(),
      }
      .into();

      // Those familiar with OpenGL are also familiar with the role of matricies
      // in graphics programming. We need to prepare a matrix to render the view
      // with. wlr_matrix_project_box is a helper which takes a box with a desired
      // x, y coordinates, width and height, and an output geometry, then
      // prepares an orthographic projection and multiplies the necessary
      // transforms to produce a model-view-projection matrix.
      //
      // Naturally you can do this any way you like, for example to make a 3D
      // compositor.
      let transform = wlr_output_transform_invert(wlr_surface.current.transform);
      let rotation = 0.0;
      let mut matrix = TransformMatrix::IDENTITY.clone();
      wlr_matrix_project_box(
        matrix.as_mut_ptr(),
        &render_box,
        transform,
        rotation,
        self.transform_matrix().as_ptr(),
      );

      // This takes our matrix, the texture, and an alpha, and performs the actual
      // rendering on the GPU.
      let alpha = 1.0;
      wlr_render_texture_with_matrix(self.renderer, texture, matrix.as_ptr(), alpha);

      // This lets the client know that we've displayed that frame and it can
      // prepare another one now if it likes.
      wlr_surface_send_frame_done(wlr_surface, frame_time);
    }
  }
}

impl Eq for Output {}
impl PartialEq for Output {
  fn eq(&self, other: &Output) -> bool {
    self.output == other.output
  }
}
impl Ord for Output {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.output.cmp(&other.output)
  }
}
impl PartialOrd for Output {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

pub(crate) trait OutputEventHandler {
  fn frame(&self);
  fn enable(&self);
  fn mode(&self);
  fn scale(&self);
  fn transform(&self);
}

impl OutputEventHandler for Rc<Output> {
  fn frame(&self) {
    self.on_frame.fire(());

    unsafe {
      // wlr_output_attach_render makes the OpenGL context current.
      if !wlr_output_attach_render(self.output, ptr::null_mut()) {
        return;
      }
      // The "effective" resolution can change if you rotate your outputs.
      let mut width: i32 = 0;
      let mut height: i32 = 0;
      wlr_output_effective_resolution(self.output, &mut width, &mut height);
      // Begin the renderer (calls glViewport and some other GL sanity checks)
      wlr_renderer_begin(self.renderer, width, height);

      let background_color = self.background_color.borrow();
      let color = [
        background_color[0],
        background_color[1],
        background_color[2],
        1.0,
      ];
      wlr_renderer_clear(self.renderer, &color[0]);

      let now = Instant::now();
      let since_creation = now.duration_since(self.created_at);
      let frame_time = timespec {
        tv_sec: since_creation.as_secs() as i64,
        tv_nsec: since_creation.subsec_nanos() as i64,
      };

      for window in self.window_manager.windows_to_render() {
        self.render_window(&frame_time, window);
      }

      // Hardware cursors are rendered by the GPU on a separate plane, and can be
      // moved around without re-rendering what's beneath them - which is more
      // efficient. However, not all hardware supports hardware cursors. For this
      // reason, wlroots provides a software fallback, which we ask it to render
      // here. wlr_cursor handles configuring hardware vs software cursors for you,
      // and this function is a no-op when hardware cursors are in use.
      wlr_output_render_software_cursors(self.output, ptr::null_mut());

      // Conclude rendering and swap the buffers, showing the final frame
      // on-screen.
      wlr_renderer_end(self.renderer);
      wlr_output_commit(self.output);
    }
  }

  fn enable(&self) {
    self.wm_policy_manager.advise_output_update(self.clone());
  }
  fn mode(&self) {
    self.wm_policy_manager.advise_output_update(self.clone());
  }
  fn scale(&self) {
    self.wm_policy_manager.advise_output_update(self.clone());
  }
  fn transform(&self) {
    self.wm_policy_manager.advise_output_update(self.clone());
  }
}

wayland_listener!(
  pub(crate) OutputEventManager,
  Weak<Output>,
  [
    frame => frame_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.frame();
      }
    };
    enable => enable_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.enable();
      }
    };
    mode => mode_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.mode();
      }
    };
    scale => scale_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.scale();
      }
    };
    transform => transform_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.transform();
      }
    };
    destroy => destroy_func: |this: &mut OutputEventManager, _data: *mut libc::c_void,| unsafe {
      if let Some(handler) = this.data.upgrade() {
        handler.on_destroy.fire(());
      }
    };
  ]
);

impl Debug for OutputEventManager {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "OutputEventManager")
  }
}

pub(crate) trait OutputEvents {
  fn bind_events(&self);
}

impl OutputEvents for Rc<Output> {
  fn bind_events(&self) {
    let mut event_manager: Pin<Box<OutputEventManager>> =
      OutputEventManager::new(Rc::downgrade(self));

    unsafe {
      event_manager.frame(&mut (*self.output).events.frame);
      event_manager.enable(&mut (*self.output).events.enable);
      event_manager.mode(&mut (*self.output).events.mode);
      event_manager.scale(&mut (*self.output).events.scale);
      event_manager.transform(&mut (*self.output).events.transform);
      event_manager.destroy(&mut (*self.output).events.destroy);
    }

    *self.event_manager.borrow_mut() = Some(event_manager);
  }
}
