use async_channel::{Receiver, Sender};
use bevy::{
    ecs::{
        bevy_utils::HashMap, ArchetypeAccess, ArchetypesGeneration, SystemId, ThreadLocalExecution,
        TypeAccess,
    },
    prelude::{Resources, System, World},
    tasks::{ComputeTaskPool, Scope},
};
use event_listener::Event;
use parking_lot::Mutex;
use std::{borrow::Cow, collections::HashSet, sync::Arc};

pub struct SchedulerSystemContainer {
    /// Boxed `bevy_ecs` system.
    pub system: Arc<Mutex<Box<dyn System>>>,
    /// Used to signal the system's task to start the system.
    pub notifier: Event,
    /// IDs of systems that depend on this one, used to decrement their dependency counters
    /// when this system finishes.
    pub dependants: Vec<SystemId>,
    /// Total amount of dependencies this system has.
    pub deps_total: usize,
    /// Amount of unsatisfied dependencies, when it reaches 0 the system is queued to be started.
    pub deps_now: usize,
}

pub struct UnorderedSchedulerSystem {
    /// Required for `System` implementation.
    pub(crate) name: Cow<'static, str>,
    /// Required for `System` implementation.
    pub(crate) id: SystemId,
    /// Required for `System` implementation.
    pub(crate) resource_access: TypeAccess,
    /// Required for `System` implementation.
    pub(crate) archetype_access: ArchetypeAccess,
    /// Used by systems to notify the scheduler that they have finished.
    pub(crate) sender: Sender<SystemId>,
    /// Used to receive finish notifications from systems.
    pub(crate) receiver: Receiver<SystemId>,
    /// Used to detect if the archetypes in the world have changed.
    pub(crate) last_archetypes_generation: ArchetypesGeneration,
    /// Systems the scheduler will be executing.
    pub(crate) system_containers: HashMap<SystemId, SchedulerSystemContainer>,
    /// Systems that should be started at next opportunity.
    pub(crate) queued: Vec<SystemId>,
    /// Systems that are currently running.
    pub(crate) running: HashSet<SystemId>,
    /// Scratch space to avoid reallocating a vector when updating dependency counters.
    pub(crate) dependants_scratch: Vec<SystemId>,
}

impl System for UnorderedSchedulerSystem {
    fn name(&self) -> Cow<'static, str> {
        self.name.clone()
    }

    fn id(&self) -> SystemId {
        self.id
    }

    fn update_archetype_access(&mut self, _: &World) {}

    fn archetype_access(&self) -> &ArchetypeAccess {
        &self.archetype_access
    }

    fn resource_access(&self) -> &TypeAccess {
        &self.resource_access
    }

    fn thread_local_execution(&self) -> ThreadLocalExecution {
        ThreadLocalExecution::Immediate
    }

    fn run(&mut self, _: &World, _: &Resources) {}

    fn run_thread_local(&mut self, world: &mut World, resources: &mut Resources) {
        self.run_systems(world, resources)
    }
}

impl UnorderedSchedulerSystem {
    /// Runs all systems.
    pub(crate) fn run_systems(&mut self, world: &mut World, resources: &mut Resources) {
        debug_assert!(self.queued.is_empty());
        debug_assert!(self.running.is_empty());
        debug_assert!(self.dependants_scratch.is_empty());
        resources
            .get_cloned::<ComputeTaskPool>()
            .unwrap()
            .scope(|scope| {
                self.prepare(scope, world, resources);
                // Spawn the scheduling task.
                scope.spawn(async {
                    // All systems have been ran if there are no queued or running systems.
                    while !(self.queued.is_empty() && self.running.is_empty()) {
                        self.start_all_runnable_queued_systems();
                        // Wait until at least one system has finished.
                        let finished = self.receiver.recv().await.unwrap();
                        self.process_finished_system(finished);
                        while let Ok(finished) = self.receiver.try_recv() {
                            self.process_finished_system(finished);
                        }
                        self.update_counters_and_queue_systems();
                    }
                })
            });
        debug_assert!(self.queued.is_empty());
        debug_assert!(self.running.is_empty());
        debug_assert!(self.dependants_scratch.is_empty());
    }

    /// Resets dependency counters, updates archetype access if needed, and spawns system tasks.
    fn prepare<'scope>(
        &mut self,
        scope: &mut Scope<'scope, ()>,
        world: &'scope World,
        resources: &'scope Resources,
    ) {
        let sender = &self.sender;
        // Reset dependency counters and spawn system tasks.
        let iterator = self.system_containers.iter_mut().map(|(&id, container)| {
            debug_assert!(container.deps_now == 0);
            container.deps_now = container.deps_total;
            let system = container.system.clone();
            let listener = container.notifier.listen();
            let sender = sender.clone();
            scope.spawn(async move {
                listener.await;
                system
                    .try_lock()
                    .unwrap_or_else(|| unreachable!())
                    .run(world, resources);
                sender.send(id).await.unwrap();
            });
            (id, container)
        });
        let should_queue_now_filter_map =
            |(id, container): (SystemId, &mut SchedulerSystemContainer)| {
                if container.deps_now == 0 {
                    Some(id)
                } else {
                    None
                }
            };
        if self.last_archetypes_generation == world.archetypes_generation() {
            // If archetypes haven't changed,
            // simply queue the systems with all dependencies satisfied.
            let should_queue_now = iterator.filter_map(should_queue_now_filter_map);
            self.queued.extend(should_queue_now);
        } else {
            // If archetypes have changed,
            let should_queue_now = iterator
                // update all systems' archetype access,
                .inspect(|(_, container)| container.system.lock().update_archetype_access(world))
                // and queue the systems with all dependencies satisfied.
                .filter_map(should_queue_now_filter_map);
            self.queued.extend(should_queue_now);
            self.last_archetypes_generation = world.archetypes_generation();
        };
    }

    /// Signals all queued systems with satisfied dependencies to start if they can, and moves
    /// them from `queued` to `running`.
    fn start_all_runnable_queued_systems(&mut self) {
        for &id in &self.queued {
            if self.can_start_now(id) {
                self.running.insert(id);
                self.system_containers
                    .get(&id)
                    .unwrap_or_else(|| unreachable!())
                    .notifier
                    .notify(1);
            }
        }
        // Remove now running systems from queued systems.
        let running = &self.running;
        self.queued.retain(|id| !running.contains(id));
    }

    /// Determines if the system with given ID can run concurrently with already running systems.
    fn can_start_now(&self, id: SystemId) -> bool {
        // TODO I hate this.
        let system = self
            .system_containers
            .get(&id)
            .unwrap_or_else(|| unreachable!())
            .system
            .lock();
        for id in &self.running {
            let other = self
                .system_containers
                .get(id)
                .unwrap_or_else(|| unreachable!())
                .system
                .lock();
            if !system
                .resource_access()
                .is_compatible(other.resource_access())
            {
                return false;
            }
            if !system
                .archetype_access()
                .is_compatible(other.archetype_access())
            {
                return false;
            }
        }
        true
    }

    /// Removes system from `running` and stores it's dependants in `dependants_scratch`.
    fn process_finished_system(&mut self, id: SystemId) {
        self.running.remove(&id);
        let container = self
            .system_containers
            .get(&id)
            .unwrap_or_else(|| unreachable!());
        self.dependants_scratch
            .extend(container.dependants.iter().cloned());
    }

    /// Decrements dependency counters for systems in `dependants_scratch` and moves the ones with
    /// satisfied dependencies to `queued`.
    fn update_counters_and_queue_systems(&mut self) {
        for id in self.dependants_scratch.drain(..) {
            let container = self
                .system_containers
                .get_mut(&id)
                .unwrap_or_else(|| unreachable!());
            container.deps_now -= 1;
            if container.deps_now == 0 {
                self.queued.push(id);
            }
        }
    }
}
