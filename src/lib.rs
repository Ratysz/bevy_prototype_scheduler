use bevy::{
    ecs::{bevy_utils::HashMap, ArchetypeAccess, ArchetypesGeneration, SystemId, TypeAccess},
    prelude::System,
};
use event_listener::Event;
use parking_lot::Mutex;
use std::{borrow::Cow, collections::HashSet, sync::Arc};

mod named_system;
mod scheduling;

pub use named_system::NamedSystem;
use scheduling::{SchedulerSystemContainer, UnorderedSchedulerSystem};

struct BuilderSystemContainer {
    system: Box<dyn System>,
    dependencies: Vec<Cow<'static, str>>,
}

/// Runs systems concurrently without inferring any order between them. Explicit order
/// between any two systems can be optionally specified via `depends_on()`.
pub struct UnorderedScheduler {
    /// Optional user-given name; defaults to "UnorderedScheduler{SystemId.0}".
    name: Option<Cow<'static, str>>,
    /// Systems the scheduler will be executing.
    system_containers: HashMap<Cow<'static, str>, BuilderSystemContainer>,
    /// Used via `depends_on` to describe that the last inserted system depends on another.
    last_added: Option<Cow<'static, str>>,
}

impl UnorderedScheduler {
    /// Create a new empty unordered scheduler.
    pub fn new() -> Self {
        UnorderedScheduler {
            name: None,
            system_containers: HashMap::default(),
            last_added: None,
        }
    }

    /// Names the scheduler; default name is "UnorderedScheduler{SystemId.0}".
    pub fn with_name<Name: Into<Cow<'static, str>>>(mut self, name: Name) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Inserts a system with it's default name.
    pub fn add_system(self, system: Box<dyn System>) -> Self {
        self.add_named_system(system.name(), system)
    }

    /// Inserts a system with a user-given name.
    pub fn add_named_system<Name: Into<Cow<'static, str>>>(
        mut self,
        name: Name,
        system: Box<dyn System>,
    ) -> Self {
        let name = name.into();
        self.last_added = Some(name.clone());
        self.system_containers.insert(
            name,
            BuilderSystemContainer {
                system,
                dependencies: Vec::new(),
            },
        );
        self
    }

    /// Specifies that the last inserted system depends on a system with the given name.
    pub fn depends_on<Name: Into<Cow<'static, str>>>(mut self, dependency_name: Name) -> Self {
        self.system_containers
            .get_mut(
                self.last_added
                    .as_ref()
                    .expect("unable to add a dependency: insert a system first"),
            )
            .unwrap()
            .dependencies
            .push(dependency_name.into());
        self
    }

    /// Finalizes the scheduler and converts it into a `bevy_ecs` system.
    pub fn into_system(mut self) -> Box<dyn System> {
        let id = SystemId::new();
        let name = self
            .name
            .unwrap_or_else(|| format!("UnorderedScheduler{}", id.0).into());
        // Cache dependencies to populate systems' dependants later.
        let mut all_dependencies = Vec::new();
        // TODO detangle the nested mapping.
        let mut ids_and_containers: HashMap<
            Cow<'static, str>,
            (SystemId, SchedulerSystemContainer),
        > = self
            .system_containers
            .drain()
            .map(|(_, container)| {
                let id = container.system.id();
                let dependencies_total = container.dependencies.len();
                all_dependencies.push((id, container.dependencies));
                (
                    container.system.name(),
                    (
                        id,
                        SchedulerSystemContainer {
                            system: Arc::new(Mutex::new(container.system)),
                            notifier: Event::new(),
                            dependants: Vec::new(),
                            deps_total: dependencies_total,
                            deps_now: 0,
                        },
                    ),
                )
            })
            .collect();
        // Populate systems' dependants lists from cached dependencies.
        for (dependant, mut dependencies) in all_dependencies.drain(..) {
            for dependee in dependencies.drain(..) {
                ids_and_containers
                    .get_mut(&dependee)
                    .unwrap_or_else(|| unreachable!())
                    .1
                    .dependants
                    .push(dependant);
            }
        }
        let (sender, receiver) = async_channel::unbounded();
        Box::new(UnorderedSchedulerSystem {
            name,
            id,
            resource_access: TypeAccess::default(),
            archetype_access: ArchetypeAccess::default(),
            sender,
            receiver,
            last_archetypes_generation: ArchetypesGeneration(u64::MAX),
            system_containers: ids_and_containers
                .drain()
                .map(|(_, id_and_container)| (id_and_container.0, id_and_container.1))
                .collect(),
            queued: Vec::new(),
            running: HashSet::new(),
            dependants_scratch: Vec::new(),
        })
    }
}
