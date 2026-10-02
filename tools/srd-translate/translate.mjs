// Translates every player-facing SRD string to Russian and stores the result in an
// append-only translation memory (data/srd/i18n/ru.tm.jsonl). Safe to stop and rerun:
// only strings missing from the memory are sent.
//
// Keys: env GEMINI_KEYS (comma separated) or tools/srd-translate/keys.txt (one per line).
// Models are never named here: the chain is rebuilt from the v1 + v1beta model lists.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const SRD = path.resolve(HERE, "../../data/srd");
const TM_PATH = path.join(SRD, "i18n/ru.tm.jsonl");
const FAILED_PATH = path.join(SRD, "i18n/ru.failed.json");
const STATUS_PATH = path.join(HERE, "status.json");
const API = "https://generativelanguage.googleapis.com/";

// The free tier allows 20 requests a day per model, so each request carries as much as one answer can hold.
const BATCH_CHARS = 30000;
const BATCH_ITEMS = 400;
const WORKERS = 2;
const MAX_ATTEMPTS = 8;
const REQUEST_TIMEOUT_MS = 600000;
// A longer wait than this means the daily quota is spent: stop and let the next run resume.
const GIVE_UP_WAIT_MS = 30 * 60 * 1000;
const MODEL_LIST_TTL_MS = 30 * 60 * 1000;

const TEXT_KEYS = new Set([
  "name", "desc", "description", "higher_level", "material", "language_desc", "age", "size_description",
  "special", "string", "notes", "note", "summary", "repeatable", "gear", "languages", "alignment", "limited-to",
  "action_name", "subtype", "duration", "casting_time", "range", "vehicle_category", "tool_category",
  "category_range", "armor_category", "weapon_category", "gear_category",
]);

const SYSTEM = `You translate the Dungeons & Dragons 5th edition System Reference Document from English to Russian for a video game.

Input: a JSON array of {"i": number, "en": string}. Output: a JSON array of {"i": number, "ru": string} with exactly the same "i" values, one per input item.

Rules:
- Translate every item fully into natural Russian. Never leave an item in English, never return an empty string.
- Use the terminology of the official Russian edition of D&D 5e. Keep one term for one concept everywhere.
- Keep every number, dice expression (1d6, 2d8 + 3) and distance value exactly as written. Do not convert feet to metres; write "футов"/"фута"/"фут".
- Keep markdown, table pipes, bullet markers, asterisks and line breaks exactly where they are.
- Transliterate proper names into Cyrillic. Short names stay short: an item that is only a name gets only a name back.
- Capitalise names of spells, classes, features and magic items the way Russian rulebooks do.

Glossary: Strength Сила; Dexterity Ловкость; Constitution Телосложение; Intelligence Интеллект; Wisdom Мудрость; Charisma Харизма; saving throw спасбросок; ability check проверка характеристики; attack roll бросок атаки; damage roll бросок урона; hit points хиты; temporary hit points временные хиты; Hit Dice Кости Хитов; Armor Class Класс Доспеха; AC КД; DC Сл; proficiency bonus бонус мастерства; proficiency владение; advantage преимущество; disadvantage помеха; spell slot ячейка заклинаний; cantrip заговор; concentration концентрация; action действие; bonus action бонусное действие; reaction реакция; short rest короткий отдых; long rest продолжительный отдых; challenge rating показатель опасности; initiative инициатива; speed скорость; darkvision тёмное зрение; resistance сопротивление; immunity иммунитет; vulnerability уязвимость; gp зм; sp см; cp мм; ep эм; pp пм.
Damage: acid кислота; bludgeoning дробящий; cold холод; fire огонь; force силовое поле; lightning электричество; necrotic некротическая энергия; piercing колющий; poison яд; psychic психическая энергия; radiant излучение; slashing рубящий; thunder звук.
Conditions: blinded ослеплённый; charmed очарованный; deafened оглохший; frightened испуганный; grappled схваченный; incapacitated недееспособный; invisible невидимый; paralyzed парализованный; petrified окаменевший; poisoned отравленный; prone сбитый с ног; restrained опутанный; stunned ошеломлённый; unconscious без сознания; exhaustion истощение.
Classes: Barbarian Варвар; Bard Бард; Cleric Жрец; Druid Друид; Fighter Воин; Monk Монах; Paladin Паладин; Ranger Следопыт; Rogue Плут; Sorcerer Чародей; Warlock Колдун; Wizard Волшебник.
Peoples: Dwarf Дварф; Elf Эльф; Halfling Полурослик; Human Человек; Dragonborn Драконорождённый; Gnome Гном; Half-Elf Полуэльф; Half-Orc Полуорк; Tiefling Тифлинг.`;

const SAFETY_OFF = ["HARM_CATEGORY_HARASSMENT", "HARM_CATEGORY_HATE_SPEECH", "HARM_CATEGORY_SEXUALLY_EXPLICIT", "HARM_CATEGORY_DANGEROUS_CONTENT"]
  .map((category) => ({ category, threshold: "OFF" }));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const now = () => Date.now();
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

function loadKeys() {
  const fromEnv = (process.env.GEMINI_KEYS || "").split(",");
  const file = path.join(HERE, "keys.txt");
  const fromFile = fs.existsSync(file) ? fs.readFileSync(file, "utf8").split(/\r?\n/) : [];
  const keys = [...new Set([...fromEnv, ...fromFile].map((k) => k.trim()).filter(Boolean))];
  if (!keys.length) throw new Error("No Gemini keys: set GEMINI_KEYS or create tools/srd-translate/keys.txt");
  return keys;
}

// ---------- source strings ----------

function readJson(file) {
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

function needsTranslation(s) {
  return /[A-Za-z]{2}/.test(s) && !/^\d+d\d+(\s*[+-]\s*\d+)?$/.test(s.trim());
}

function collectSources() {
  const seen = new Set();
  const out = [];
  const walk = (v, key) => {
    if (typeof v === "string") {
      if (TEXT_KEYS.has(key) && needsTranslation(v) && !seen.has(v)) {
        seen.add(v);
        out.push(v);
      }
    } else if (Array.isArray(v)) {
      for (const x of v) walk(x, key);
    } else if (v && typeof v === "object") {
      for (const [k, x] of Object.entries(v)) walk(x, k);
    }
  };
  for (const edition of ["2014", "2024"]) {
    const dir = path.join(SRD, edition, "en");
    for (const f of fs.readdirSync(dir).sort()) walk(readJson(path.join(dir, f)), "");
  }
  return out;
}

// The upstream database ships a human Russian translation for a few small files; reuse it.
function seedPairs() {
  const pairs = [];
  const seedDir = path.join(SRD, "2014/ru-seed");
  if (!fs.existsSync(seedDir)) return pairs;
  for (const f of fs.readdirSync(seedDir)) {
    const enFile = path.join(SRD, "2014/en", f);
    if (!fs.existsSync(enFile)) continue;
    const ru = new Map(readJson(path.join(seedDir, f)).map((e) => [e.index, e]));
    for (const en of readJson(enFile)) {
      const r = ru.get(en.index);
      if (!r) continue;
      for (const key of ["name", "full_name"]) {
        if (typeof en[key] === "string" && typeof r[key] === "string") pairs.push([en[key], r[key]]);
      }
      const a = [].concat(en.desc ?? []);
      const b = [].concat(r.desc ?? []);
      if (a.length === b.length) a.forEach((s, i) => pairs.push([s, b[i]]));
    }
  }
  return pairs;
}

// ---------- translation memory ----------

const tm = new Map();

function loadTm() {
  if (!fs.existsSync(TM_PATH)) return;
  for (const line of fs.readFileSync(TM_PATH, "utf8").split("\n")) {
    if (!line.trim()) continue;
    try {
      const e = JSON.parse(line);
      tm.set(e.src, e.ru);
    } catch {
      // A torn last line from an interrupted run: the string simply gets translated again.
    }
  }
}

function remember(src, ru, model, warn) {
  tm.set(src, ru);
  fs.appendFileSync(TM_PATH, JSON.stringify({ src, ru, model, ...(warn ? { warn } : {}) }) + "\n");
}

const numbersOf = (s) => new Set(s.match(/\d+/g) || []);

// Returns null when the translation is acceptable, otherwise the reason it is not.
function reject(src, ru) {
  if (typeof ru !== "string" || !ru.trim()) return "empty";
  if (!/[А-Яа-яЁё]/.test(ru)) return "no-cyrillic";
  if (src.length >= 20 && (ru.length < src.length * 0.4 || ru.length > src.length * 3.5)) return "length";
  const got = numbersOf(ru);
  for (const n of numbersOf(src)) if (!got.has(n)) return "numbers";
  return null;
}

// ---------- models and keys ----------

const EXCLUDE = /lite|image|tts|live|audio|transcribe|embedding|robotics|computer-use|omni|customtools|latest/;

function versionOf(name) {
  const m = name.match(/^gemini-(\d+)(?:\.(\d+))?-flash/);
  return m ? [Number(m[1]), Number(m[2] || 0)] : [0, 0];
}

class Pool {
  constructor(keys) {
    this.keys = keys;
    this.deadKeys = new Set();
    this.models = [];
    this.listedAt = 0;
    this.modelCooldown = new Map();
    this.pairCooldown = new Map();
    this.strikes = new Map();
    this.thinking = new Map();
  }

  async refresh(force) {
    if (!force && this.models.length && now() - this.listedAt < MODEL_LIST_TTL_MS) return;
    for (const key of this.keys) {
      if (this.deadKeys.has(key)) continue;
      const names = new Set();
      try {
        for (const v of ["v1", "v1beta"]) {
          const r = await fetch(`${API}${v}/models?pageSize=1000`, { headers: { "x-goog-api-key": key } });
          if (!r.ok) throw new Error(`list ${v} HTTP ${r.status}`);
          for (const m of (await r.json()).models || []) {
            const n = m.name.replace("models/", "");
            if ((m.supportedGenerationMethods || []).includes("generateContent") && /^gemini-[\d.]+-flash/.test(n) && !EXCLUDE.test(n)) names.add(n);
          }
        }
      } catch (e) {
        log("model list failed:", e.message);
        continue;
      }
      // Newest first; at equal version the stable name goes before its preview.
      this.models = [...names].sort((a, b) => {
        const [a1, a2] = versionOf(a);
        const [b1, b2] = versionOf(b);
        return b1 - a1 || b2 - a2 || Number(a.includes("preview")) - Number(b.includes("preview"));
      });
      this.listedAt = now();
      log("chain:", this.models.join(" > "));
      return;
    }
    if (!this.models.length) throw new Error("Could not list models with any key");
  }

  // Best model that is free right now, with a key that is free for it; or how long to wait.
  pick() {
    let soonest = Infinity;
    for (const model of this.models) {
      const mc = this.modelCooldown.get(model) || 0;
      if (mc > now()) {
        soonest = Math.min(soonest, mc);
        continue;
      }
      for (const key of this.keys) {
        if (this.deadKeys.has(key)) continue;
        const pc = this.pairCooldown.get(key + "|" + model) || 0;
        if (pc <= now()) return { model, key };
        soonest = Math.min(soonest, pc);
      }
    }
    return { waitMs: soonest === Infinity ? 60000 : Math.max(1000, soonest - now()) };
  }

  strike(id, baseMs, capMs) {
    const n = (this.strikes.get(id) || 0) + 1;
    this.strikes.set(id, n);
    return Math.min(capMs, baseMs * 2 ** (n - 1));
  }

  ok(model, key) {
    this.strikes.delete(model);
    this.strikes.delete(key + "|" + model);
  }

  // Newer models take a thinking level, older ones a token budget, some neither: found out by asking.
  thinkingConfig(model) {
    const mode = this.thinking.get(model) || "level";
    if (mode === "level") return { thinkingLevel: "low" };
    if (mode === "budget") return { thinkingBudget: 1024 };
    return undefined;
  }

  downgradeThinking(model) {
    const mode = this.thinking.get(model) || "level";
    if (mode === "none") return false;
    this.thinking.set(model, mode === "level" ? "budget" : "none");
    return true;
  }
}

function retryDelayMs(error) {
  for (const d of error?.details || []) {
    const m = typeof d.retryDelay === "string" && d.retryDelay.match(/^([\d.]+)s$/);
    if (m) return Math.ceil(Number(m[1]) * 1000);
  }
  return 0;
}

function quotaIds(error) {
  const ids = [];
  for (const d of error?.details || []) for (const v of d.violations || []) ids.push(`${v.quotaId}=${v.quotaValue ?? "?"}`);
  return ids.join(", ");
}

function parseItems(text) {
  try {
    const j = JSON.parse(text);
    if (Array.isArray(j)) return j;
  } catch {
    // Truncated output: keep every item that arrived complete.
  }
  const out = [];
  const re = /\{\s*"i"\s*:\s*(\d+)\s*,\s*"ru"\s*:\s*("(?:[^"\\]|\\.)*")\s*\}/g;
  for (let m; (m = re.exec(text || "")); ) {
    try {
      out.push({ i: Number(m[1]), ru: JSON.parse(m[2]) });
    } catch {
      // Skip an item whose string does not decode.
    }
  }
  return out;
}

async function requestBatch(pool, model, key, items) {
  const tc = pool.thinkingConfig(model);
  const body = {
    systemInstruction: { parts: [{ text: SYSTEM }] },
    contents: [{ role: "user", parts: [{ text: JSON.stringify(items.map((en, i) => ({ i, en }))) }] }],
    safetySettings: SAFETY_OFF,
    generationConfig: {
      temperature: 0.2,
      responseMimeType: "application/json",
      responseSchema: { type: "ARRAY", items: { type: "OBJECT", properties: { i: { type: "INTEGER" }, ru: { type: "STRING" } }, required: ["i", "ru"] } },
      ...(tc ? { thinkingConfig: tc } : {}),
    },
  };
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), REQUEST_TIMEOUT_MS);
  try {
    const r = await fetch(`${API}v1beta/models/${model}:generateContent`, {
      method: "POST",
      headers: { "x-goog-api-key": key, "content-type": "application/json" },
      body: JSON.stringify(body),
      signal: ctl.signal,
    });
    const j = await r.json().catch(() => ({}));
    const parts = j.candidates?.[0]?.content?.parts || [];
    return {
      status: r.status,
      error: j.error,
      finish: j.candidates?.[0]?.finishReason,
      block: j.promptFeedback?.blockReason,
      text: parts.filter((p) => !p.thought).map((p) => p.text || "").join(""),
    };
  } catch (e) {
    return { status: 0, error: { message: e.name === "AbortError" ? "timeout" : e.message } };
  } finally {
    clearTimeout(timer);
  }
}

// ---------- main ----------

const keys = loadKeys();
fs.mkdirSync(path.dirname(TM_PATH), { recursive: true });
loadTm();
for (const [src, ru] of seedPairs()) if (!tm.has(src) && needsTranslation(src) && !reject(src, ru)) remember(src, ru, "seed");

const sources = collectSources();
const attempts = new Map();
const failed = new Map();
let queue = sources.filter((s) => !tm.has(s));
const totalChars = sources.reduce((n, s) => n + s.length, 0);
log(`strings ${sources.length} (${Math.round(totalChars / 1000)}k chars), already translated ${sources.length - queue.length}, to do ${queue.length}`);

function takeBatch() {
  const batch = [];
  let chars = 0;
  while (queue.length && batch.length < BATCH_ITEMS) {
    const next = queue[0];
    // Strings that already failed go alone, so one bad item cannot spoil a batch again.
    const lonely = (attempts.get(next) || 0) >= 2;
    if (batch.length && (lonely || chars + next.length > BATCH_CHARS)) break;
    batch.push(queue.shift());
    chars += next.length;
    if (lonely) break;
  }
  return batch;
}

function status(extra) {
  const done = sources.filter((s) => tm.has(s)).length;
  fs.writeFileSync(STATUS_PATH, JSON.stringify({ at: new Date().toISOString(), total: sources.length, done, queued: queue.length, failed: failed.size, ...extra }, null, 2));
  return done;
}

const pool = new Pool(keys);
await pool.refresh(true);
let inFlight = 0;

async function worker(id) {
  for (;;) {
    if (!queue.length) {
      if (inFlight === 0) return;
      await sleep(2000);
      continue;
    }
    await pool.refresh(false);
    const pick = pool.pick();
    if (pick.waitMs) {
      if (pick.waitMs > GIVE_UP_WAIT_MS && inFlight === 0) {
        log(`worker ${id}: every model is out of quota for ${Math.round(pick.waitMs / 60000)} min, stopping`);
        return;
      }
      await sleep(Math.min(pick.waitMs, 60000));
      continue;
    }
    const batch = takeBatch();
    if (!batch.length) continue;
    const { model, key } = pick;
    const pair = key + "|" + model;
    // Hold the pair while the request runs so the other workers spread over other models and keys.
    pool.pairCooldown.set(pair, now() + REQUEST_TIMEOUT_MS);
    inFlight++;
    const res = await requestBatch(pool, model, key, batch);
    inFlight--;
    pool.pairCooldown.set(pair, 0);

    if (res.status !== 200) {
      queue = batch.concat(queue);
      const msg = String(res.error?.message || "").slice(0, 140);
      if (res.status === 429) {
        const wait = Math.max(retryDelayMs(res.error), pool.strike(pair, 60000, 60 * 60 * 1000));
        pool.pairCooldown.set(pair, now() + wait);
        log(`429 ${model} key#${keys.indexOf(key)} wait ${Math.round(wait / 1000)}s ${quotaIds(res.error)}`);
      } else if (res.status === 400 && /thinking/i.test(msg) && pool.downgradeThinking(model)) {
        log(`${model}: thinking mode -> ${pool.thinking.get(model)}`);
      } else if (res.status === 404) {
        log(`404 ${model}: gone, relisting`);
        await pool.refresh(true);
      } else if ((res.status === 400 || res.status === 401 || res.status === 403) && /api key|permission|credential/i.test(msg)) {
        pool.deadKeys.add(key);
        log(`key#${keys.indexOf(key)} rejected: ${msg}`);
        if (pool.deadKeys.size === keys.length) throw new Error("Every key was rejected");
      } else {
        // Overload, server error, timeout: a model problem, another key would not help.
        const wait = pool.strike(model, 45000, 10 * 60 * 1000);
        pool.modelCooldown.set(model, now() + wait);
        log(`${res.status || "net"} ${model} wait ${Math.round(wait / 1000)}s ${msg}`);
      }
      continue;
    }

    pool.ok(model, key);
    const got = new Map(parseItems(res.text).map((x) => [x.i, x.ru]));
    let saved = 0;
    const back = [];
    batch.forEach((src, i) => {
      const ru = got.get(i);
      const why = reject(src, ru);
      const n = (attempts.get(src) || 0) + 1;
      if (!why) {
        remember(src, ru.trim(), model);
        saved++;
      } else if (why === "numbers" && n >= 3) {
        // Number words ("two" vs "2") trip the check on honest translations; keep it, flagged for review.
        remember(src, ru.trim(), model, "numbers");
        saved++;
      } else if (n >= MAX_ATTEMPTS) {
        failed.set(src, why);
      } else {
        attempts.set(src, n);
        back.push(src);
      }
    });
    queue = queue.concat(back);
    const done = status({ model });
    log(`${model} batch ${batch.length}: saved ${saved}, retry ${back.length}${res.finish && res.finish !== "STOP" ? " finish=" + res.finish : ""}${res.block ? " block=" + res.block : ""} | ${done}/${sources.length}`);
  }
}

await Promise.all(Array.from({ length: WORKERS }, (_, i) => worker(i)));
fs.writeFileSync(FAILED_PATH, JSON.stringify([...failed].map(([src, why]) => ({ src, why })), null, 2));
const done = status({ finished: queue.length === 0 });
log(`stopped: ${done}/${sources.length} translated, ${queue.length} still queued, ${failed.size} failed (see ${path.relative(process.cwd(), FAILED_PATH)})`);
