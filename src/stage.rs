use downcast_rs::Downcast;
use std::fmt::Debug;

use crate::{Resources, World};

pub trait Stage: Debug + Downcast {
    fn run(&mut self, world: &mut World, resources: &mut Resources);
}

downcast_rs::impl_downcast!(Stage);
