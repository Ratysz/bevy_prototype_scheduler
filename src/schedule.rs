use std::{
    collections::HashMap,
    fmt::{Debug, Formatter, Result as FmtResult},
};

use crate::{Resources, Stage, StageLabel, World};

#[derive(Default)]
pub struct Schedule {
    stages: Vec<Box<dyn Stage>>,
    index_table: HashMap<Box<dyn StageLabel>, usize>,
}

impl Schedule {
    pub fn new() -> Self {
        Default::default()
    }

    fn stage_index(&self, label: &impl StageLabel) -> Option<usize> {
        self.index_table.get(label as &dyn StageLabel).cloned()
    }

    fn insert_stage(&mut self, stage_index: usize, stage: impl Stage) {
        self.stages.insert(stage_index, Box::new(stage));
        for index in self
            .index_table
            .values_mut()
            .filter(|index| **index >= stage_index)
        {
            *index += 1;
        }
    }

    fn remove_stage<S: Stage>(&mut self, stage_index: usize) -> S {
        let stage = self.stages.remove(stage_index);
        for index in self
            .index_table
            .values_mut()
            .filter(|index| **index > stage_index)
        {
            *index -= 1;
        }
        *stage.downcast::<S>().map_err(|_| ()).unwrap()
    }

    fn insert_label(&mut self, stage_index: usize, label: impl StageLabel) {
        assert!(!self.index_table.contains_key(&label as &dyn StageLabel));
        self.index_table.insert(Box::new(label), stage_index);
    }

    fn remove_label(&mut self, label: &impl StageLabel) -> Option<usize> {
        self.index_table.remove(label as &dyn StageLabel)
    }

    pub fn add(&mut self, label: impl StageLabel, stage: impl Stage) {
        self.insert_label(self.stages.len(), label);
        self.stages.push(Box::new(stage));
    }

    pub fn add_before(
        &mut self,
        target_label: &impl StageLabel,
        label: impl StageLabel,
        stage: impl Stage,
    ) {
        let index = self.stage_index(target_label).unwrap();
        self.insert_stage(index, stage);
        self.insert_label(index, label);
    }

    pub fn add_after(
        &mut self,
        target_label: &impl StageLabel,
        label: impl StageLabel,
        stage: impl Stage,
    ) {
        let index = self.stage_index(target_label).unwrap() + 1;
        self.insert_stage(index, stage);
        self.insert_label(index, label);
    }

    pub fn remove<S: Stage>(&mut self, label: &impl StageLabel) -> Option<S> {
        self.remove_label(label)
            .map(move |index| self.remove_stage(index))
    }

    pub fn stage_mut<S: Stage>(&mut self, label: &impl StageLabel) -> Option<&mut S> {
        self.index_table
            .get(label as &dyn StageLabel)
            .cloned()
            .and_then(move |index| self.stages[index].downcast_mut())
    }

    pub fn run(&mut self, world: &mut World, resources: &mut Resources) {
        for stage in &mut self.stages {
            stage.run(world, resources);
        }
    }
}

impl Debug for Schedule {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        let mut index_table = self.index_table.iter().collect::<Vec<_>>();
        index_table.sort_by(|(_, index), (_, other_index)| index.cmp(other_index));
        f.debug_map()
            .entries(
                index_table
                    .drain(..)
                    .map(|(label, index)| (label, &self.stages[*index])),
            )
            .finish()
    }
}
