use bevy::ecs::{IntoForEachSystem, IntoQuerySystem};
use core::any::type_name;

/// Allows fetching a system's default name without converting it into a boxed system.
/// Automatically implemented for all not thread local systems.
pub trait NamedSystem<Marker, Generics> {
    /// Get the name of the system.
    fn name(&self) -> &'static str;
}

pub struct ForEach;

impl<T, CommandBuffer, R, Q> NamedSystem<ForEach, (CommandBuffer, R, Q)> for T
where
    T: IntoForEachSystem<CommandBuffer, R, Q>,
{
    fn name(&self) -> &'static str {
        type_name::<Self>()
    }
}

pub struct Query;

impl<T, Commands, R, Q> NamedSystem<Query, (Commands, R, Q)> for T
where
    T: IntoQuerySystem<Commands, R, Q>,
{
    fn name(&self) -> &'static str {
        type_name::<Self>()
    }
}

/*pub struct ThreadLocal;

impl<T> NamedSystem<ThreadLocal, ()> for T
where
    T: IntoThreadLocalSystem,
{
    fn name(&self) -> &'static str {
        type_name::<Self>()
    }
}*/
