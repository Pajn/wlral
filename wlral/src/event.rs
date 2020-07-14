use std::{cell::RefCell, collections::BTreeMap, fmt::Debug, rc::Rc};

type EventListener<Data> = Box<dyn Fn(&Data)>;

pub struct Event<Data> {
  next_id: RefCell<u64>,
  listeners: RefCell<BTreeMap<u64, Rc<EventListener<Data>>>>,
}

impl<T> Debug for Event<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Event")
  }
}

impl<T> Default for Event<T> {
  fn default() -> Self {
    Event {
      next_id: RefCell::new(0),
      listeners: RefCell::new(BTreeMap::new()),
    }
  }
}

impl<T> Event<T> {
  pub fn subscribe(&self, handler: EventListener<T>) -> u64 {
    let id = self.next_id.borrow().clone();
    *self.next_id.borrow_mut() = id + 1;
    self.listeners.borrow_mut().insert(id, Rc::new(handler));
    id
  }
  pub fn unsubscribe(&self, id: u64) {
    self.listeners.borrow_mut().remove(&id);
  }

  pub fn fire(&self, data: T) {
    for listener in self.listeners.borrow().values() {
      listener(&data);
    }
  }
}

type EventListenerOnce<Data> = Box<dyn FnOnce(&Data)>;

pub struct EventOnce<Data> {
  listeners: RefCell<Vec<EventListenerOnce<Data>>>,
}

impl<T> Debug for EventOnce<T> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "EventOnce")
  }
}

impl<T> Default for EventOnce<T> {
  fn default() -> Self {
    EventOnce {
      listeners: RefCell::new(vec![]),
    }
  }
}

impl<T> EventOnce<T> {
  pub fn then(&self, handler: EventListenerOnce<T>) {
    self.listeners.borrow_mut().push(handler);
  }

  pub fn fire(&self, data: T) {
    while let Some(listener) = self.listeners.borrow_mut().pop() {
      listener(&data);
    }
  }
}
