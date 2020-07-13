use crate::{event::Event, input::keyboard::KeyboardConfig};
use log::debug;
use std::{cell::RefCell, rc::Rc};

#[derive(Default, Debug, Eq, PartialEq, Clone)]
pub struct Config {
  pub keyboard: KeyboardConfig,
}

pub struct ConfigManager {
  config: RefCell<Rc<Config>>,
  on_config_changed: Event<Rc<Config>>,
}

impl ConfigManager {
  pub fn new() -> ConfigManager {
    ConfigManager {
      config: RefCell::new(Rc::new(Config::default())),
      on_config_changed: Event::default(),
    }
  }

  pub fn config(&self) -> Rc<Config> {
    self.config.borrow().clone()
  }

  pub fn update_config<F>(&self, mut updater: F)
  where
    F: FnMut(&mut Config),
  {
    let mut config = self.config.borrow().clone();
    updater(Rc::make_mut(&mut config));
    *self.config.borrow_mut() = config;
    debug!("ConfigManager::updated_config");
    self.on_config_changed.fire(self.config.borrow().clone());
  }

  pub fn on_config_changed(&self) -> &Event<Rc<Config>> {
    &self.on_config_changed
  }
}
