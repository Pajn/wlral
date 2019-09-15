use std::ops::{Add, Mul, Sub};
use wlroots_sys::wlr_box;

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct TPoint<T: Copy> {
  pub x: T,
  pub y: T,
}

pub type Point = TPoint<i32>;
pub type FPoint = TPoint<f64>;

impl<T: Copy> TPoint<T> {
  pub fn x(&self) -> T {
    self.x
  }

  pub fn y(&self) -> T {
    self.y
  }

  pub fn as_displacement(&self) -> TDisplacement<T> {
    TDisplacement {
      dx: self.x,
      dy: self.y,
    }
  }
}

impl From<Point> for FPoint {
  fn from(point: Point) -> Self {
    FPoint {
      x: point.x as f64,
      y: point.y as f64,
    }
  }
}

impl From<FPoint> for Point {
  fn from(point: FPoint) -> Self {
    Point {
      x: point.x as i32,
      y: point.y as i32,
    }
  }
}

impl Mul<f32> for Point {
  type Output = Point;

  fn mul(self, other: f32) -> Point {
    Point {
      x: ((self.x as f32) * other) as i32,
      y: ((self.y as f32) * other) as i32,
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub struct Size {
  pub width: i32,
  pub height: i32,
}

impl Size {
  pub fn width(&self) -> i32 {
    self.width
  }

  pub fn height(&self) -> i32 {
    self.height
  }

  pub fn with_width(&self, width: i32) -> Size {
    Size {
      width,
      height: self.height,
    }
  }

  pub fn with_height(&self, height: i32) -> Size {
    Size {
      width: self.width,
      height,
    }
  }
}

impl Add<Size> for Size {
  type Output = Size;

  fn add(self, other: Size) -> Self::Output {
    Size {
      width: self.width + other.width,
      height: self.height + other.height,
    }
  }
}

impl Sub<Size> for Size {
  type Output = Size;

  fn sub(self, other: Size) -> Self::Output {
    Size {
      width: self.width - other.width,
      height: self.height - other.height,
    }
  }
}

impl Mul<f32> for Size {
  type Output = Size;

  fn mul(self, other: f32) -> Size {
    Size {
      width: ((self.width as f32) * other) as i32,
      height: ((self.height as f32) * other) as i32,
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct Rectangle {
  pub top_left: Point,
  pub size: Size,
}

impl Rectangle {
  pub fn left(&self) -> i32 {
    self.top_left.x
  }

  pub fn top(&self) -> i32 {
    self.top_left.y
  }

  pub fn right(&self) -> i32 {
    self.left() + self.width()
  }

  pub fn bottom(&self) -> i32 {
    self.top() + self.height()
  }

  pub fn width(&self) -> i32 {
    self.size.width
  }

  pub fn height(&self) -> i32 {
    self.size.height
  }

  pub fn top_left(&self) -> Point {
    Point {
      x: self.left(),
      y: self.top(),
    }
  }

  pub fn bottom_right(&self) -> Point {
    Point {
      x: self.right(),
      y: self.bottom(),
    }
  }

  pub fn contains(&self, point: &Point) -> bool {
    self.left() <= point.x
      && self.right() > point.x
      && self.top() <= point.y
      && self.bottom() > point.y
  }
}

impl From<wlr_box> for Rectangle {
  fn from(wlr_box: wlr_box) -> Self {
    Rectangle {
      top_left: Point {
        x: wlr_box.x,
        y: wlr_box.y,
      },
      size: Size {
        width: wlr_box.width,
        height: wlr_box.height,
      },
    }
  }
}

impl From<Rectangle> for wlr_box {
  fn from(rect: Rectangle) -> Self {
    wlr_box {
      x: rect.top_left.x,
      y: rect.top_left.y,
      width: rect.size.width,
      height: rect.size.height,
    }
  }
}

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct TDisplacement<T> {
  pub dx: T,
  pub dy: T,
}

pub type Displacement = TDisplacement<i32>;
pub type FDisplacement = TDisplacement<f64>;

impl<T: Copy> Sub<TPoint<T>> for TPoint<T>
where
  T: Sub<T, Output = T>,
{
  type Output = TDisplacement<T>;

  fn sub(self, other: Self) -> Self::Output {
    TDisplacement {
      dx: self.x - other.x,
      dy: self.y - other.y,
    }
  }
}

impl<T: Copy> Add<TDisplacement<T>> for TPoint<T>
where
  T: Add<T, Output = T>,
{
  type Output = TPoint<T>;

  fn add(self, other: TDisplacement<T>) -> Self::Output {
    TPoint {
      x: self.x + other.dx,
      y: self.y + other.dy,
    }
  }
}

impl<T: Copy> Sub<TDisplacement<T>> for TPoint<T>
where
  T: Sub<T, Output = T>,
{
  type Output = TPoint<T>;

  fn sub(self, other: TDisplacement<T>) -> Self::Output {
    TPoint {
      x: self.x - other.dx,
      y: self.y - other.dy,
    }
  }
}

#[repr(transparent)]
#[derive(Debug, Default, PartialEq, Clone)]
pub struct TransformMatrix(pub [f32; 9]);

impl TransformMatrix {
  pub const IDENTITY: TransformMatrix =
    TransformMatrix([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]);

  pub fn as_ptr(&self) -> *const f32 {
    self.0.as_ptr()
  }

  pub fn as_mut_ptr(&mut self) -> *mut f32 {
    self.0.as_mut_ptr()
  }
}
