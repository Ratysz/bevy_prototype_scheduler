use bevy_prototype_scheduler::{Resources, Schedule, Stage, StageLabel, World};

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
struct StructLabel(usize);

impl StageLabel for StructLabel {}

#[derive(PartialEq, Eq, Hash, Clone, Debug)]
enum EnumLabel {
    A,
    B,
}

impl StageLabel for EnumLabel {}

#[derive(Debug)]
struct PrintyStage(&'static str);

impl Stage for PrintyStage {
    fn run(&mut self, _: &mut World, _: &mut Resources) {
        println!("{}", self.0);
    }
}

fn main() {
    let mut world = World;
    let mut resources = Resources;
    let mut schedule = Schedule::new();
    schedule.add(StructLabel(0), PrintyStage("I am struct label 0"));
    schedule.add(StructLabel(1), PrintyStage("I am struct label 1"));
    schedule.add_before(
        &StructLabel(1),
        EnumLabel::A,
        PrintyStage("I am enum label A"),
    );
    schedule.add_after(
        &EnumLabel::A,
        EnumLabel::B,
        PrintyStage("I am enum label B"),
    );
    schedule.run(&mut world, &mut resources);
    println!("{:#?}", schedule);
}
