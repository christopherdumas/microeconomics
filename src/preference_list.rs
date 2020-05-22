use crate::items::discretes::Goal;
use crate::items::discretes::Item;
use std::cmp::{Ord, Ordering};
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::rc::Rc;

/// Contains all of the metadata required to satisfy a goal properly. This data
/// is stored only in the preference list of the actor and the recurrance list
/// of the actor, since the preference list is the data structure that is
/// actually used when satisfying goals, and the recurrance list is the only
/// place where the metadata about recurrance time intervals matter. I could
/// have designed separate data structures for those two peices of information,
/// but that would've been unweildy in my opinion.
#[derive(PartialEq, Eq, Clone, Copy)]
pub enum GoalData {
    /// A goal that either occurs at random times or only once.
    Satisfaction {
        /// The goal to be satisfied
        goal: Goal,
        /// Amount of acceptable units needed to satisfy this goal
        units_required: i32,
        /// Current units diverted to this goal
        units: i32,
        /// Unique id
        id: i32,
    },
    /// A regularly recurring goal.
    RegularSatisfaction {
        /// The goal to be satisfied
        goal: Goal,
        /// Time required for this goal to reoccur
        time_required: i32,
        /// Time since this goal was dismissed
        time: i32,
        /// Amount of acceptable units needed to satisfy this goal
        units_required: i32,
        /// Current units diverted to this goal
        units: i32,
        /// Unique id
        id: i32,
    },
}

impl GoalData {
    /// Get the goal this metadata might satisfy
    pub fn get_goal(&self) -> Goal {
        match self {
            &GoalData::Satisfaction { goal, .. } | &GoalData::RegularSatisfaction { goal, .. } => {
                goal
            }
        }
    }

    /// Check if this goal should be in the recurrance list
    pub fn is_recurring(&self) -> bool {
        match self {
            &GoalData::Satisfaction { .. } => false,
            _ => true,
        }
    }
}

/// This is necessary to take advantage of the automatic sorting abilities of
/// the BinaryHeap that we use in the preference list. This only exists because
/// of that, there's nothing special about this otherwise.
pub struct GoalWrapper {
    /// Closure that encloses a reference-counted pointer to the goal hierarchy
    /// of the containing actor so it can do comparasons.
    comparator: Box<dyn Fn(&GoalData, &GoalData) -> Ordering>,
    /// The actual interesting data that we want the BinaryHeap to sort
    pub goal: GoalData,
}

impl PartialOrd for GoalWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for GoalWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.goal == other.goal
    }
}

impl Eq for GoalWrapper {}

impl Ord for GoalWrapper {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.comparator)(&self.goal, &other.goal)
    }
}

/// A map of the item that must be valued or used to the max-heap containing the
/// goals that can be satisfied with the item. Since the most highly-valued goal
/// is the one that will always be referenced for both use and valuing, those
/// operations need only ever deal with the root of the heap, making this very
/// performant.
pub type PreferenceList = HashMap<Item, BinaryHeap<GoalWrapper>>;

/// Individual acting, valuing, satisfying Austrian microeconomic actor
pub struct Actor {
    /// Name for printouts
    pub name: String,
    /// Goals that might show up later, so we need to cache their information
    recurring_goals: HashMap<Goal, GoalData>,
    /// Mapping of items to their goals
    pub preference_list: PreferenceList,
    /// Mapping of goals to the items that can satisfy them
    satisfactions: HashMap<Goal, Vec<Item>>,
    // TODO: Make sure that goal heirarchy is strictly ordinal.
    /// How much goals are valued. This could easily be stored as a list, and in
    /// fact is constructed from one, but is more performant for our purposes as
    /// a map from a goal to how much it is valued.
    pub goal_hierarchy: HashMap<Goal, usize>,
}

impl Actor {
    /// Construct a new actor. Does some housekeeping to make construction easier.
    ///
    /// # Arguments
    ///
    /// * `name` - actor's name, for printout results
    /// * `hierarchy` - list of actor's valued ends as `GoalData` so that they can also be added to other places.
    ///
    pub fn new(
        name: String,
        hierarchy: Vec<GoalData>,
        satisfactions: Vec<(Goal, Vec<Item>)>,
    ) -> Self {
        let mut this = Actor {
            name: name,
            recurring_goals: HashMap::new(),
            preference_list: HashMap::new(),
            satisfactions: satisfactions.into_iter().collect(),
            goal_hierarchy: HashMap::new(),
        };
        for (i, goal) in hierarchy.into_iter().enumerate() {
            this.add_goal(goal, i);
        }
        this
    }

    /// Adds a goal to all of the BinaryHeaps for all of the items that can satisfy it (sorted).
    ///
    /// # Arguments
    ///
    /// * `goal` - `GoalData` of what's to be added
    /// * `location` - the location for it to be inserted into the hierarchy of ends/values
    ///
    pub fn add_goal(&mut self, goal: GoalData, location: usize) {
        let actual_goal = goal.get_goal();
        if let Some(effected_entries) = self.satisfactions.get(&actual_goal) {
            for item in effected_entries.iter() {
                {
                    let gh = self.goal_hierarchy.clone();
                    let ordered_goal = GoalWrapper {
                        comparator: Box::new(move |x: &GoalData, y: &GoalData| {
                            let xval = gh.get(&x.get_goal());
                            let yval = gh.get(&y.get_goal());
                            xval.and_then(|x| yval.map(|y| x.cmp(y)))
                                .unwrap_or(Ordering::Equal)
                        }),
                        goal: goal,
                    };
                    let mut goals = BinaryHeap::new();
                    goals.push(ordered_goal);
                    self.preference_list
                        .entry(*item)
                        .or_insert(BinaryHeap::new())
                        .append(&mut goals);
                }
            }
        }
        if goal.is_recurring() {
            self.recurring_goals.insert(goal.get_goal(), goal.clone());
        }
        self.goal_hierarchy.insert(goal.get_goal(), location);
    }

    /// Removes any goal in the entire list of goals this actor has.
    ///
    /// # Arguments
    ///
    /// * `actual_goal` - The goal (not `GoalData` or `GoalWrapper`) to remove
    ///
    /// # Notes
    ///
    /// Since items are always used for the highest-valued goal which they can
    /// satisfy (and thus the base node in the BinaryHeap), `pop()` would
    /// suffice in the small case. That would be ideal because it would be very
    /// fast. However, for goals that can be satisfied by multiple items, which
    /// might be the highest valued goal that can be satisfied by some items but
    /// not by others, we need to be more complex. This method is an extreme
    /// performance basket-case and should basically never be used unless
    /// absolutely totally necessary
    ///
    pub fn remove_goal(&mut self, actual_goal: Goal) {
        if let Some(effected_entries) = self.satisfactions.get(&actual_goal) {
            for item in effected_entries.iter() {
                {
                    if self.preference_list.contains_key(&item) {
                        let mut new = BinaryHeap::new();
                        self.preference_list
                            .get(&item)
                            .map(|goals: &BinaryHeap<GoalWrapper>| {
                                for og in goals.into_iter() {
                                    if og.goal.get_goal() != actual_goal {
                                        let gh = self.goal_hierarchy.clone();
                                        new.push(GoalWrapper {
                                            comparator: Box::new(
                                                move |x: &GoalData, y: &GoalData| {
                                                    let xval = gh.get(&x.get_goal());
                                                    let yval = gh.get(&y.get_goal());
                                                    xval.and_then(|x| yval.map(|y| x.cmp(y)))
                                                        .unwrap_or(Ordering::Equal)
                                                },
                                            ),
                                            goal: og.goal,
                                        });
                                    }
                                }
                            });
                        *self.preference_list.get_mut(&item).unwrap() = new;
                    }
                }
            }
        }
        self.recurring_goals.remove(&actual_goal);
        self.goal_hierarchy.remove(&actual_goal);
    }

    /// Uses an item to satisfy the most valued goal it can satisfy.
    ///
    /// # Arguments
    ///
    /// * `item` - `Item` to use
    ///
    /// # Notes
    ///
    /// Doesn't update recurring goals. See `tick`.
    ///
    pub fn use_item(&mut self, item: Item) -> Option<GoalData> {
        if let Some(goals) = self.preference_list.get_mut(&item) {
            if let Some(wrapper) = goals.peek() {
                let highest_valued_goal: GoalData = wrapper.goal;
                match highest_valued_goal {
                    GoalData::Satisfaction {
                        goal,
                        units_required,
                        mut units,
                        ..
                    } => {
                        units += 1;
                        if units >= units_required {
                            self.remove_goal(goal);
                            Some(highest_valued_goal)
                        } else {
                            None
                        }
                    }
                    GoalData::RegularSatisfaction {
                        goal,
                        units_required,
                        mut units,
                        ..
                    } => {
                        units += 1;
                        if units >= units_required {
                            self.remove_goal(goal);
                            Some(highest_valued_goal)
                        } else {
                            None
                        }
                    }
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Add an item to the list of items that can satisfy a given goal.
    ///
    /// # Arguments
    ///
    /// * `goal` - the goal that can be satisfied with this item
    /// * `item` - the item that can satisfy this goal
    ///
    pub fn add_satisfaction_entry(&mut self, goal: Goal, item: Item) {
        self.satisfactions
            .entry(goal)
            .or_insert(vec![item])
            .push(item);
    }

    /// Get the highest-valued goal which can be satisfied with this item
    ///
    /// # Arguments
    ///
    /// * `item` - the item
    ///
    pub fn get_best_goal(&self, item: Item) -> Option<Goal> {
        self.preference_list
            .get(&item)
            .and_then(|goals| goals.peek())
            .map(|og| og.goal.get_goal())
    }

    /// Compare two items to see which is more valuable based on the goals it can satisfy
    ///
    /// # Arguments
    ///
    /// * `a` - first item
    /// * `b` - second item
    ///
    pub fn compare_item_values(&self, a: Item, b: Item) -> Option<Ordering> {
        self.get_best_goal(a)
            .and_then(|a_g| self.get_best_goal(b).map(|b_g| a_g.cmp(&b_g)))
    }
}
