//! The rules of the table as the master reads them: what the host chose for this world and
//! how the game itself settles checks, fights, rest and growth. The same text goes to the
//! model when the master asks it for a ruling, so that the answer is about this game.

use crate::i18n::{Lang, T};
use world::{Difficulty, World};

/// The book, as sections: a title and its text.
pub fn book(world: &World, lang: Lang) -> Vec<(String, String)> {
    let t = &world.table;
    let yes = |on: bool| if on { T::RbYes.text(lang).to_string() } else { T::RbNo.text(lang).to_string() };
    let difficulty = match t.difficulty {
        Difficulty::Casual => T::RbCasualABeatenParty.text(lang).to_string(),
        Difficulty::Honest => T::RbHonestAHeroCan.text(lang).to_string(),
        Difficulty::Hardcore => T::RbHardcoreDeathIsFinal.text(lang).to_string(),
    };
    let method = match t.method {
        rules::AbilityMethod::PointBuy => T::RbPointBuy.text(lang).to_string(),
        rules::AbilityMethod::StandardArray => T::RbStandardArray.text(lang).to_string(),
        rules::AbilityMethod::Roll => T::RbDTheLowestDie.text(lang).to_string(),
    };
    // The template is translated whole; its blanks are filled here.
    let table = T::RbTableTemplate
        .text(lang)
        .replace("{world}", &world.title)
        .replace("{edition}", if t.srd_2024 { T::RbEdition2024.text(lang) } else { "2014" })
        .replace("{difficulty}", &difficulty)
        .replace("{mode}", if t.sandbox { T::RbSandbox.text(lang) } else { T::RbStory.text(lang) })
        .replace("{book}", &yes(t.book_only))
        .replace("{limits}", &yes(t.no_limits))
        .replace("{method}", &method)
        .replace("{live}", &yes(t.live))
        .replace("{violence}", &yes(world.violence))
        .replace("{sex}", &yes(world.sex));
    let checks = T::RbAnOutcomeThatIs.text(lang).to_string();
    let fight = T::RbTheGameRunsA.text(lang).to_string();
    let rest = T::RbAShortRestA.text(lang).to_string();
    let growth = T::RbExperienceComesFromVictories.text(lang).to_string();
    let things = T::RbEachHeroHasA.text(lang).to_string();
    let worldly = T::RbEveryPlaceHasA.text(lang).to_string();
    let mut sections = vec![
        (T::RbThisTable.text(lang).to_string(), table),
        (T::RbChecks.text(lang).to_string(), checks),
        (T::RbFights.text(lang).to_string(), fight),
        (T::RbRest.text(lang).to_string(), rest),
        (T::RbExperienceAndLevels.text(lang).to_string(), growth),
        (T::RbThingsAndTheBag.text(lang).to_string(), things),
        (T::RbTheWorld.text(lang).to_string(), worldly),
    ];
    if world.quest.is_set() {
        let quest = &world.quest;
        let mut text = format!("{}\n{}: {}", quest.title, T::RbGoal.text(lang).to_string(), quest.goal);
        for (n, act) in quest.acts.iter().enumerate() {
            text.push_str(&format!("\n{}. {act}", n + 1));
        }
        if !quest.ending.is_empty() {
            text.push_str(&format!("\n{}: {}", T::RbWon.text(lang).to_string(), quest.ending));
        }
        if !quest.failure.is_empty() {
            text.push_str(&format!("\n{}: {}", T::RbLost.text(lang).to_string(), quest.failure));
        }
        sections.push((T::RbTheMainQuestThe.text(lang).to_string(), text));
    }
    let party: Vec<String> = world
        .party
        .iter()
        .map(|h| format!("{} — {} {} {}, {} {}/{}, {} {}", h.name, h.race, h.class, h.level, T::RbHp.text(lang).to_string(), h.hp, h.max_hp, T::RbAc.text(lang).to_string(), h.ac()))
        .collect();
    if !party.is_empty() {
        sections.push((T::RbTheParty.text(lang).to_string(), party.join("\n")));
    }
    sections
}

/// The book as one text, for the model.
pub fn text(world: &World, lang: Lang) -> String {
    book(world, lang).into_iter().map(|(title, body)| format!("## {title}\n{body}")).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_book_tells_this_tables_choices() {
        let draft: world::WorldDraft = serde_json::from_str(&world::fake::world("rules")).unwrap();
        let mut world = world::build_world(&draft, "rules", "English").unwrap().0;
        world.table.difficulty = Difficulty::Hardcore;
        world.table.srd_2024 = true;
        let text = text(&world, Lang::En);
        assert!(text.contains("Hardcore"));
        assert!(text.contains("2024"));
        assert!(text.contains("death save"));
    }
}
