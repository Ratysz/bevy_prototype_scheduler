use bevy::prelude::*;
use bevy_prototype_scheduler::{NamedSystem as _, UnorderedScheduler};

fn main() {
    App::build()
        .add_resource(bevy::tasks::ComputeTaskPool(
            bevy::tasks::TaskPoolBuilder::new().num_threads(1).build(),
        ))
        .add_system(
            UnorderedScheduler::new()
                .add_system(one.system())
                .depends_on(two.name())
                .add_system(two.system())
                .into_system(),
        )
        .run();
}

fn one(_: Query<(&usize, &mut f32)>) {
    println!("Hello from system one!");
}

fn two(_: Query<&usize>) {
    println!("Hello from system two!");
}
