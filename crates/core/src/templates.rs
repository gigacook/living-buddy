//! Built-in routine templates. They are copied into the database on first run
//! so households can edit them; these are only the starting point.

use crate::model::{Category, RepeatMode};

pub struct RoutineTemplate {
    pub key: &'static str,
    pub title: &'static str,
    pub category: Category,
    pub rrule: &'static str,
    pub repeat_mode: RepeatMode,
    pub duration_minutes: u32,
    pub checklist: &'static [&'static str],
    pub tip: &'static str,
}

pub const TEMPLATES: &[RoutineTemplate] = &[
    RoutineTemplate {
        key: "exercise",
        title: "Gym / exercise",
        category: Category::Personal,
        rrule: "FREQ=WEEKLY;BYDAY=MO,WE,FR",
        repeat_mode: RepeatMode::Fixed,
        duration_minutes: 45,
        checklist: &["Pack bag and water", "Warm up", "Main session", "Stretch"],
        tip: "A short walk counts too. Showing up is the win.",
    },
    RoutineTemplate {
        key: "groceries",
        title: "Grocery shopping",
        category: Category::Errands,
        rrule: "FREQ=WEEKLY;BYDAY=SA",
        repeat_mode: RepeatMode::Fixed,
        duration_minutes: 60,
        checklist: &["Check the fridge and pantry", "Write the list", "Bring bags", "Put everything away"],
        tip: "Keep a running list on the fridge so the trip itself is easy.",
    },
    RoutineTemplate {
        key: "waste",
        title: "Waste and recycling",
        category: Category::Home,
        rrule: "FREQ=WEEKLY;BYDAY=TU",
        repeat_mode: RepeatMode::Fixed,
        duration_minutes: 10,
        checklist: &["Empty kitchen bin", "Sort recycling", "Take bins out", "Fresh bin bag"],
        tip: "Match this to your local collection day.",
    },
    RoutineTemplate {
        key: "cleaning",
        title: "General cleaning",
        category: Category::Home,
        rrule: "FREQ=WEEKLY",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 30,
        checklist: &["Clear surfaces", "Quick dust", "Empty small bins"],
        tip: "Set a 15-minute timer and stop when it rings.",
    },
    RoutineTemplate {
        key: "dishes",
        title: "Dishes",
        category: Category::Home,
        rrule: "FREQ=DAILY",
        repeat_mode: RepeatMode::Fixed,
        duration_minutes: 15,
        checklist: &["Load or wash", "Run the dishwasher", "Unload"],
        tip: "Unloading the dishwasher first makes the rest flow.",
    },
    RoutineTemplate {
        key: "organizing",
        title: "Organizing",
        category: Category::Home,
        rrule: "FREQ=WEEKLY;INTERVAL=2",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 20,
        checklist: &["Pick one small area", "Keep / donate / bin", "Put things back where they live"],
        tip: "One drawer is a complete job.",
    },
    RoutineTemplate {
        key: "laundry",
        title: "Laundry",
        category: Category::Home,
        rrule: "FREQ=WEEKLY;BYDAY=SU",
        repeat_mode: RepeatMode::Fixed,
        duration_minutes: 30,
        checklist: &["Sort and start a load", "Move to dryer or rack", "Fold", "Put away"],
        tip: "Set a timer when the machine starts so clothes don't sit wet.",
    },
    RoutineTemplate {
        key: "vacuuming",
        title: "Vacuuming",
        category: Category::Home,
        rrule: "FREQ=WEEKLY",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 20,
        checklist: &["Pick things up off the floor", "Vacuum main rooms", "Empty the vacuum"],
        tip: "Music makes it go faster.",
    },
    RoutineTemplate {
        key: "mopping",
        title: "Mopping floors",
        category: Category::Home,
        rrule: "FREQ=WEEKLY;INTERVAL=2",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 25,
        checklist: &["Vacuum first", "Mop kitchen", "Mop bathroom", "Rinse the mop"],
        tip: "Kitchen and bathroom first; the rest can wait.",
    },
    RoutineTemplate {
        key: "surfaces",
        title: "Wiping surfaces",
        category: Category::Home,
        rrule: "FREQ=DAILY;INTERVAL=2",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 10,
        checklist: &["Kitchen counters", "Table", "Stove top"],
        tip: "Keep a cloth where you'll see it.",
    },
    RoutineTemplate {
        key: "bathroom",
        title: "Cleaning the bathroom",
        category: Category::Home,
        rrule: "FREQ=WEEKLY",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 30,
        checklist: &["Toilet", "Sink and mirror", "Shower or tub", "Fresh towels", "Floor"],
        tip: "Spray first, then let it soak while you do the mirror.",
    },
    RoutineTemplate {
        key: "bed_linen",
        title: "Changing bed linen",
        category: Category::Home,
        rrule: "FREQ=WEEKLY;INTERVAL=2",
        repeat_mode: RepeatMode::AfterCompletion,
        duration_minutes: 20,
        checklist: &["Strip the bed", "Start the wash", "Put on fresh linen"],
        tip: "Keep a spare set so the bed is never waiting on laundry.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recurrence::RRule;

    #[test]
    fn templates_are_valid() {
        assert_eq!(TEMPLATES.len(), 12);
        let mut keys: Vec<&str> = TEMPLATES.iter().map(|t| t.key).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 12);
        for t in TEMPLATES {
            RRule::parse(t.rrule).unwrap_or_else(|e| panic!("{}: {e}", t.key));
            assert!(!t.checklist.is_empty());
        }
    }
}
