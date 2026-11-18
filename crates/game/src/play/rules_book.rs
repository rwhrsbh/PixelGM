//! The rules of the table as the master reads them: what the host chose for this world and
//! how the game itself settles checks, fights, rest and growth. The same text goes to the
//! model when the master asks it for a ruling, so that the answer is about this game.

use crate::i18n::Lang;
use world::{Difficulty, World};

/// The book, as sections: a title and its text.
pub fn book(world: &World, lang: Lang) -> Vec<(String, String)> {
    let ru = lang == Lang::Ru;
    let pick = |r: &str, e: &str| if ru { r.to_string() } else { e.to_string() };
    let t = &world.table;
    let yes = |on: bool| if on { pick("да", "yes") } else { pick("нет", "no") };
    let difficulty = match t.difficulty {
        Difficulty::Casual => pick("Лёгкая: побеждённый отряд приходит в себя с 1 хитом, еда для отдыха не нужна.", "Casual: a beaten party wakes with 1 hit point; no food is needed to rest."),
        Difficulty::Honest => pick("Честная: герой может умереть; для долгого отдыха каждому нужна еда.", "Honest: a hero can die; every hero needs food for a long rest."),
        Difficulty::Hardcore => pick("Хардкор: смерть окончательна, спасбросок от смерти один; еда для отдыха нужна.", "Hardcore: death is final and there is one death save; food is needed to rest."),
    };
    let method = match t.method {
        rules::AbilityMethod::PointBuy => pick("покупка очков (27)", "point buy (27)"),
        rules::AbilityMethod::StandardArray => pick("стандартный набор 15, 14, 13, 12, 10, 8", "standard array 15, 14, 13, 12, 10, 8"),
        rules::AbilityMethod::Roll => pick("бросок 4d6, худший куб отбрасывается", "4d6, the lowest die dropped"),
    };
    let table = if ru {
        format!(
            "Мир: {}.\nРедакция правил: SRD 5e {}.\nСложность: {difficulty}\nРежим: {}.\nТолько то, что есть в книге (монстры SRD, без выдуманных умений): {}.\nОграничения щедрого мастера сняты: {}.\nХарактеристики: {method}.\nЖивой мир (мастер действует и в фоне): {}.\nНасилие подробно: {}. Откровенные сцены: {}.",
            world.title,
            if t.srd_2024 { "2024 (бета)" } else { "2014" },
            if t.sandbox { "песочница — без главного задания" } else { "сюжет — есть главное задание" },
            yes(t.book_only),
            yes(t.no_limits),
            yes(t.live),
            yes(world.violence),
            yes(world.sex)
        )
    } else {
        format!(
            "World: {}.\nRules edition: SRD 5e {}.\nDifficulty: {difficulty}\nMode: {}.\nOnly what the book has (SRD monsters, no invented abilities): {}.\nThe caps on a generous master lifted: {}.\nAbility scores: {method}.\nA living world (the master acts in the background too): {}.\nGraphic violence: {}. Explicit scenes: {}.",
            world.title,
            if t.srd_2024 { "2024 (beta)" } else { "2014" },
            if t.sandbox { "sandbox, no main quest" } else { "story, with a main quest" },
            yes(t.book_only),
            yes(t.no_limits),
            yes(t.live),
            yes(world.violence),
            yes(world.sex)
        )
    };
    let checks = pick(
        "Исход, который не ясен, решает проверка: d20 + модификатор характеристики (+ бонус мастерства, если герой владеет навыком) против сложности (СЛ). СЛ 10 — легко, 15 — средне, 20 — трудно, 25 — почти невозможно. Преимущество: бросаются два d20, берётся больший; помеха — меньший. Мастер не бросает за игроков: он назначает характеристику, навык и СЛ, исход решают кубы. Пассивная внимательность (10 + Мудрость + мастерство) замечает спрятанное, когда герой проходит рядом.",
        "An outcome that is not certain is settled by a check: d20 + the ability modifier (+ the proficiency bonus when the hero is trained in the skill) against a difficulty class (DC). DC 10 is easy, 15 medium, 20 hard, 25 nearly impossible. Advantage: roll two d20 and take the higher; disadvantage, the lower. The master does not roll for the players: they name the ability, the skill and the DC, and the dice decide. Passive perception (10 + Wisdom + proficiency) notices what is hidden when a hero walks by.",
    );
    let fight = pick(
        "Бой ведёт игра, удар за ударом. Инициатива: d20 + Ловкость. В свой ход существо проходит до своей скорости (5 футов = клетка) и делает одно действие: атаку, заклинание, рывок (вдвое дальше), уклонение (атаки по нему с помехой), отход (без атак при уходе), предмет. Атака: d20 + бонус против КД цели; 20 на кубе — критическое попадание, кости урона удваиваются; 1 — промах. Помеха: цель дальше обычной дальности, стрелок стоит рядом с врагом, цель уклоняется. По беспомощной цели вплотную — преимущество и крит при попадании. Кто уходит из досягаемости врага без отхода, получает атаку при возможности. На 0 хитов герой падает и в свой ход бросает спасбросок от смерти: 10+ — успех, ниже — провал, 1 — два провала, 20 — встаёт с 1 хитом; три успеха — стабилен, три провала — смерть. Урон по упавшему — провал.",
        "The game runs a fight blow by blow. Initiative: d20 + Dexterity. On its turn a creature moves up to its speed (5 feet = a cell) and takes one action: an attack, a spell, a dash (twice as far), a dodge (attacks on it have disadvantage), a disengage (no attacks as it leaves), an item. An attack: d20 + bonus against the target's AC; a natural 20 is a critical hit and the damage dice are doubled; a 1 misses. Disadvantage: the target is beyond normal range, the shooter stands beside an enemy, the target dodges. Against a helpless target in reach: advantage, and a hit is critical. Whoever leaves an enemy's reach without disengaging takes an opportunity attack. At 0 hit points a hero falls and on its turn makes a death save: 10+ succeeds, below fails, a 1 is two failures, a 20 stands up with 1 hit point; three successes — stable, three failures — dead.",
    );
    let rest = pick(
        "Короткий отдых: герой тратит кости хитов (их столько, сколько уровней) и лечится на бросок + Телосложение. Долгий отдых: только там, где нет врагов, и (кроме лёгкой сложности) у каждого есть еда — она съедается; хиты и ячейки заклинаний восстанавливаются полностью, возвращается половина костей хитов.",
        "A short rest: a hero spends hit dice (as many as their levels) and heals the roll + Constitution. A long rest: only where nothing hostile is, and (except on casual) with food for each hero, which is eaten; hit points and spell slots come back in full, and half the hit dice.",
    );
    let growth = pick(
        "Опыт даётся за победы (опыт врагов делится на отряд) и за дела. Уровни: 300, 900, 2 700, 6 500, 14 000 опыта и далее по SRD. На новом уровне растут хиты и умения класса; мастер может предложить герою особые умения.",
        "Experience comes from victories (the enemies' XP shared by the party) and from deeds. Levels: 300, 900, 2,700, 6,500, 14,000 XP and on as in the SRD. A new level brings hit points and class features; the master may offer a hero abilities of their own.",
    );
    let things = pick(
        "У каждого героя свой рюкзак-сетка: вещь занимает клетки по размеру, её можно повернуть (R). Пользоваться можно только вещами из своего рюкзака; передать вещь можно, только стоя рядом. Вещи на земле подбирают, стоя на клетке или рядом. Торговцы продают по своим ценам и пополняют товар каждый день; золото общее для покупки только у того, кто платит.",
        "Each hero has a bag of their own on a grid: a thing takes cells by its size and can be turned (R). Only things in one's own bag can be used; a thing is handed over only by standing side by side. Things on the ground are picked up from their cell or beside it. Merchants sell at their prices and restock daily; the one who buys pays from their own gold.",
    );
    let worldly = pick(
        "У каждого места есть опасность от 0 (город, лагерь: никто не приходит) до 5 (логово). Если она выше 0, каждый час игра бросает, не забредёт ли кто-то или не случится ли встреча. Фракции помнят, кто им помог и кто навредил (отношение от −3 до +3). Нити — дела, что вернутся позже; угрозы — враги со своими планами, они созревают по часам. Спутников можно уговорить, нанять или получить в награду.",
        "Every place has a danger from 0 (a town, a camp: nobody comes) to 5 (a lair). Above 0 the game rolls each hour for a wanderer or an encounter. Factions remember who helped and who wronged them (standing from −3 to +3). Threads are deeds that come back later; threats are enemies with plans of their own that ripen by the clock. Companions can be talked round, hired or given as a reward.",
    );
    let mut sections = vec![
        (pick("Этот стол", "This table"), table),
        (pick("Проверки", "Checks"), checks),
        (pick("Бой", "Fights"), fight),
        (pick("Отдых", "Rest"), rest),
        (pick("Опыт и уровни", "Experience and levels"), growth),
        (pick("Вещи и рюкзак", "Things and the bag"), things),
        (pick("Мир", "The world"), worldly),
    ];
    if world.quest.is_set() {
        let quest = &world.quest;
        let mut text = format!("{}\n{}: {}", quest.title, pick("Цель", "Goal"), quest.goal);
        for (n, act) in quest.acts.iter().enumerate() {
            text.push_str(&format!("\n{}. {act}", n + 1));
        }
        if !quest.ending.is_empty() {
            text.push_str(&format!("\n{}: {}", pick("Победа", "Won"), quest.ending));
        }
        if !quest.failure.is_empty() {
            text.push_str(&format!("\n{}: {}", pick("Поражение", "Lost"), quest.failure));
        }
        sections.push((pick("Главное задание (только для мастера)", "The main quest (the master's alone)"), text));
    }
    let party: Vec<String> = world
        .party
        .iter()
        .map(|h| format!("{} — {} {} {}, {} {}/{}, {} {}", h.name, h.race, h.class, h.level, pick("хиты", "HP"), h.hp, h.max_hp, pick("КД", "AC"), h.ac()))
        .collect();
    if !party.is_empty() {
        sections.push((pick("Отряд", "The party"), party.join("\n")));
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
