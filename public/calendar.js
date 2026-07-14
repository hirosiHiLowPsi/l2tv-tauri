const titleElement = document.getElementById("calendar-title");
const summaryElement = document.getElementById("calendar-summary");
const refreshButton = document.getElementById("calendar-refresh");
const errorElement = document.getElementById("calendar-error");
const yearTitle = document.getElementById("calendar-year-title");
const monthLabels = document.getElementById("month-labels");
const weekdayLabels = document.getElementById("weekday-labels");
const heatmapLayout = document.getElementById("heatmap-layout");
const heatmapGrid = document.getElementById("heatmap-grid");
const yearButtons = document.getElementById("calendar-years");
const legacyNote = document.getElementById("legacy-history-note");
const selectedDateTitle = document.getElementById("selected-date-title");
const selectedDateSummary = document.getElementById("selected-date-summary");
const historyRows = document.getElementById("history-rows");
const historyEmpty = document.getElementById("history-empty");
const embeddedCalendarMode =
  new URLSearchParams(window.location.search).get("embedded") === "1"
  || window.frameElement?.classList.contains("beatoraja-calendar-frame") === true;

document.body.classList.toggle("calendar-embedded", embeddedCalendarMode);

const state = {
  year: new Date().getFullYear(),
  selectedDate: "",
  language: "ja",
  theme: "l2tv-pop",
  data: null,
  busy: false,
};

const labels = {
  ja: {
    title: "プレイ履歴カレンダー",
    loading: "scoredatalog.db を読み込んでいます。",
    reload: "再読込",
    less: "少",
    more: "多",
    playsInYear: (plays, days, year) => `${year}年: ${plays.toLocaleString()}プレイ / ${days.toLocaleString()}日`,
    selectDate: "日付を選択してください",
    selectDateHelp: "カレンダーの日付を選択してください。",
    daySummary: (plays, charts) => `${plays.toLocaleString()}プレイ / ${charts.toLocaleString()}譜面`,
    noHistory: "この日の履歴はありません。",
    legacy: "このscoredatalog.dbは旧形式です。譜面ごとの最新履歴のみ残るため、日別件数は実際のプレイ数より少ない場合があります。",
    headings: ["時刻", "Title", "Lamp", "EX/Rate", "BP", "使用オプション"],
    weekdays: ["日", "月", "火", "水", "木", "金", "土"],
    months: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
    dateLabel: (date) => `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日`,
    cellLabel: (date, count) => `${date.getFullYear()}年${date.getMonth() + 1}月${date.getDate()}日: ${count}プレイ`,
    unknown: "不明",
  },
  en: {
    title: "Play History Calendar",
    loading: "Loading scoredatalog.db.",
    reload: "Reload",
    less: "Less",
    more: "More",
    playsInYear: (plays, days, year) => `${plays.toLocaleString()} plays across ${days.toLocaleString()} days in ${year}`,
    selectDate: "Select a date",
    selectDateHelp: "Select a date in the calendar.",
    daySummary: (plays, charts) => `${plays.toLocaleString()} plays / ${charts.toLocaleString()} charts`,
    noHistory: "No history was recorded on this date.",
    legacy: "This is a legacy scoredatalog.db. It keeps only the latest record per chart, so daily totals may be lower than the actual play count.",
    headings: ["Time", "Title", "Lamp", "EX/Rate", "BP", "Play Option"],
    weekdays: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"],
    months: ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"],
    dateLabel: (date) => date.toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric" }),
    cellLabel: (date, count) => `${date.toLocaleDateString("en-US")}: ${count} plays`,
    unknown: "Unknown",
  },
};

refreshButton.addEventListener("click", () => {
  void loadYear(state.year, { selectLatest: Boolean(state.selectedDate) });
});

window.addEventListener("l2tv-calendar-source-changed", () => {
  state.selectedDate = "";
  void initialize();
});

void initialize();

if (embeddedCalendarMode && typeof ResizeObserver === "function") {
  const resizeObserver = new ResizeObserver(() => syncEmbeddedCalendarHeight());
  resizeObserver.observe(document.documentElement);
}

async function initialize() {
  let data = await fetchCalendarData({ year: new Date().getFullYear() });
  if (!data) {
    return;
  }
  const newestYear = Number(data.availableYears?.[0]);
  if (data.days.length === 0 && Number.isInteger(newestYear) && newestYear !== data.year) {
    data = await fetchCalendarData({ year: newestYear });
    if (!data) {
      return;
    }
  }
  applyCalendarData(data);
  const initialDate = data.latestDate || "";
  if (initialDate) {
    await loadDay(initialDate);
  }
}

async function loadYear(year, options = {}) {
  const data = await fetchCalendarData({ year });
  if (!data) {
    return;
  }
  applyCalendarData(data);
  const nextDate = options.selectLatest === false ? "" : data.latestDate || "";
  if (nextDate) {
    await loadDay(nextDate);
  } else {
    clearDayDetails();
  }
}

async function loadDay(date) {
  const data = await fetchCalendarData({ year: state.year, date });
  if (!data) {
    return;
  }
  state.selectedDate = data.selectedDate || date;
  state.data = { ...state.data, ...data };
  renderHeatmap();
  renderDayDetails(data);
}

async function fetchCalendarData(options) {
  if (state.busy) {
    return null;
  }
  setBusy(true);
  clearError();
  try {
    if (embeddedCalendarMode) {
      const source = window.parent?.l2tvEmbeddedCalendarSource;
      if (!source?.scoreDbPath) {
        throw new Error("beatoraja score.db の参照情報を取得できませんでした。");
      }
      if (typeof window.lr2irDesktop?.requestApi !== "function") {
        throw new Error("Tauri bridge is unavailable.");
      }
      const data = await window.lr2irDesktop.requestApi("/api/beatoraja-history", {
        ...options,
        scoreDbPath: source.scoreDbPath,
        songDbPath: source.songDbPath ?? "",
      });
      return {
        ...data,
        language: source.language,
        theme: source.theme,
      };
    }
    if (typeof window.lr2irDesktop?.getBeatorajaCalendarData !== "function") {
      throw new Error("Tauri bridge is unavailable.");
    }
    return await window.lr2irDesktop.getBeatorajaCalendarData(options);
  } catch (error) {
    showError(error instanceof Error ? error.message : String(error));
    return null;
  } finally {
    setBusy(false);
  }
}

function applyCalendarData(data) {
  state.year = Number(data.year) || state.year;
  state.selectedDate = "";
  state.language = data.language === "en" ? "en" : "ja";
  state.theme = data.theme === "lr2ir-dark" ? "lr2ir-dark" : "l2tv-pop";
  state.data = data;
  document.documentElement.lang = state.language;
  document.body.dataset.theme = state.theme;
  applyStaticLabels();
  renderYearButtons();
  renderHeatmap();
  legacyNote.classList.toggle("hidden", !data.legacyLatestOnly);
  legacyNote.textContent = currentLabels().legacy;
  syncEmbeddedCalendarHeight();
}

function syncEmbeddedCalendarHeight() {
  if (!embeddedCalendarMode || !window.frameElement) {
    return;
  }
  window.requestAnimationFrame(() => {
    const height = Math.max(560, Math.min(1050, document.documentElement.scrollHeight));
    window.frameElement.style.height = `${height}px`;
  });
}

function applyStaticLabels() {
  const text = currentLabels();
  titleElement.textContent = text.title;
  refreshButton.textContent = text.reload;
  document.getElementById("legend-less").textContent = text.less;
  document.getElementById("legend-more").textContent = text.more;
  document.title = `L2TV - ${text.title}`;
  const headingIds = ["heading-time", "heading-title", "heading-lamp", null, null, "heading-option"];
  headingIds.forEach((id, index) => {
    if (id) {
      document.getElementById(id).textContent = text.headings[index];
    }
  });
}

function renderYearButtons() {
  yearButtons.replaceChildren();
  const years = Array.isArray(state.data?.availableYears) ? state.data.availableYears : [];
  const renderedYears = years.length > 0 ? years : [state.year];
  const fragment = document.createDocumentFragment();
  for (const year of renderedYears) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = String(year);
    button.classList.toggle("active", Number(year) === state.year);
    button.addEventListener("click", () => {
      if (Number(year) !== state.year) {
        void loadYear(Number(year));
      }
    });
    fragment.append(button);
  }
  yearButtons.append(fragment);
}

function renderHeatmap() {
  const data = state.data ?? { days: [] };
  const text = currentLabels();
  const dayMap = new Map((data.days ?? []).map((day) => [day.date, day]));
  const maxCount = Math.max(0, ...[...dayMap.values()].map((day) => Number(day.playCount) || 0));
  const dates = buildHeatmapDates(state.year);
  const weekCount = Math.ceil(dates.length / 7);
  heatmapLayout.style.setProperty("--week-count", String(weekCount));
  yearTitle.textContent = String(state.year);
  summaryElement.textContent = text.playsInYear(Number(data.totalPlays) || 0, dayMap.size, state.year);

  monthLabels.replaceChildren();
  const seenMonths = new Set();
  for (let index = 0; index < dates.length; index += 7) {
    const week = dates.slice(index, index + 7);
    const inYear = week.find((date) => date.getFullYear() === state.year && !seenMonths.has(date.getMonth()));
    if (!inYear) {
      continue;
    }
    seenMonths.add(inYear.getMonth());
    const label = document.createElement("span");
    label.textContent = text.months[inYear.getMonth()];
    label.style.gridColumn = String(Math.floor(index / 7) + 1);
    monthLabels.append(label);
  }

  weekdayLabels.replaceChildren(...text.weekdays.map((weekday) => {
    const label = document.createElement("span");
    label.textContent = weekday;
    return label;
  }));

  const fragment = document.createDocumentFragment();
  for (const date of dates) {
    const dateKey = formatDateKey(date);
    const day = dayMap.get(dateKey);
    const count = Number(day?.playCount) || 0;
    const cell = document.createElement("button");
    cell.type = "button";
    cell.className = "heatmap-cell";
    cell.dataset.date = dateKey;
    cell.dataset.level = String(heatLevel(count, maxCount));
    cell.classList.toggle("outside-year", date.getFullYear() !== state.year);
    cell.classList.toggle("selected", dateKey === state.selectedDate);
    cell.setAttribute("role", "gridcell");
    cell.setAttribute("aria-label", text.cellLabel(date, count));
    cell.title = text.cellLabel(date, count);
    if (date.getFullYear() === state.year) {
      cell.addEventListener("click", () => void loadDay(dateKey));
    }
    fragment.append(cell);
  }
  heatmapGrid.replaceChildren(fragment);
}

function renderDayDetails(data) {
  const text = currentLabels();
  const date = parseDateKey(data.selectedDate);
  selectedDateTitle.textContent = date ? text.dateLabel(date) : text.selectDate;
  const day = (data.days ?? []).find((item) => item.date === data.selectedDate);
  selectedDateSummary.textContent = text.daySummary(Number(day?.playCount) || 0, Number(day?.chartCount) || 0);
  historyRows.replaceChildren();
  const entries = Array.isArray(data.entries) ? data.entries : [];
  historyEmpty.classList.toggle("hidden", entries.length > 0);
  historyEmpty.textContent = entries.length > 0 ? "" : text.noHistory;
  const fragment = document.createDocumentFragment();
  for (const entry of entries) {
    fragment.append(createHistoryRow(entry));
  }
  historyRows.append(fragment);
}

function createHistoryRow(entry) {
  const row = document.createElement("tr");
  row.append(createTextCell(entry.time || "-"));

  const titleCell = document.createElement("td");
  const title = document.createElement("span");
  title.className = "history-title";
  title.textContent = entry.title || "Unknown Chart";
  titleCell.append(title);
  if (entry.artist) {
    const artist = document.createElement("span");
    artist.className = "history-artist";
    artist.textContent = entry.artist;
    titleCell.append(artist);
  }
  row.append(titleCell);

  const lampCell = document.createElement("td");
  const lamp = document.createElement("span");
  lamp.className = `lamp-badge lamp-${String(entry.lampStatus || "").toLowerCase().replaceAll(" ", "-")}`;
  lamp.textContent = lampLabel(entry.lampStatus);
  lampCell.append(lamp);
  row.append(lampCell);

  const scoreCell = document.createElement("td");
  if (Number.isFinite(entry.exScore) && Number.isFinite(entry.maxExScore)) {
    scoreCell.append(document.createTextNode(`${entry.exScore.toLocaleString()} / ${entry.maxExScore.toLocaleString()}`));
    const rate = document.createElement("span");
    rate.className = "history-rate";
    rate.textContent = `${Number(entry.scoreRate || 0).toFixed(2)}%`;
    scoreCell.append(rate);
  } else {
    scoreCell.textContent = "-";
  }
  row.append(scoreCell);
  row.append(createTextCell(Number.isFinite(entry.missCount) ? String(entry.missCount) : "-"));

  const optionCell = document.createElement("td");
  const optionLabel = document.createElement("span");
  optionLabel.className = "history-option-label";
  optionLabel.textContent = formatOption(entry.playOption);
  optionCell.append(optionLabel);
  const layout = keyboardLayoutForEntry(entry);
  if (Number(entry.keyMode) === 7 && isValidLayout(layout)) {
    optionCell.append(createKeyboard(layout));
  }
  row.append(optionCell);
  return row;
}

function keyboardLayoutForEntry(entry) {
  if (Number(entry.keyMode) !== 7) {
    return "";
  }
  const savedLayout = String(entry.randomLayout || "");
  if (isValidLayout(savedLayout)) {
    return savedLayout;
  }
  const option = String(entry.playOption || "NONE").toUpperCase();
  if (option === "NONE") {
    return "1234567";
  }
  if (option === "MIRROR") {
    return "7654321";
  }
  return "";
}

function createTextCell(value) {
  const cell = document.createElement("td");
  cell.textContent = value;
  return cell;
}

function createKeyboard(layout) {
  const keyboard = document.createElement("span");
  keyboard.className = "history-keyboard";
  keyboard.setAttribute("aria-label", `RANDOM Layout: ${layout}`);
  for (const lane of layout) {
    const image = document.createElement("img");
    image.src = `/assets/keys/key_${lane}.png`;
    image.alt = "";
    image.width = 64;
    image.height = 76;
    image.draggable = false;
    image.setAttribute("aria-hidden", "true");
    keyboard.append(image);
  }
  return keyboard;
}

function formatOption(value) {
  const normalized = String(value || "NONE").toUpperCase();
  const names = state.language === "en"
    ? { NONE: "NORMAL", MIRROR: "MIRROR", RANDOM: "RANDOM", "R-RANDOM": "R-RANDOM", "S-RANDOM": "S-RANDOM", "H-RANDOM": "H-RANDOM" }
    : { NONE: "正規", MIRROR: "鏡", RANDOM: "乱", "R-RANDOM": "R乱", "S-RANDOM": "S乱", "H-RANDOM": "H乱" };
  return names[normalized] || normalized || currentLabels().unknown;
}

function lampLabel(value) {
  const labels = {
    "FULL COMBO": "FC",
    "EX HARD CLEAR": "EXHARD",
    "HARD CLEAR": "HARD",
    CLEAR: "NORMAL",
    "EASY CLEAR": "EASY",
    "NO PLAY": "NO PLAY",
  };
  return labels[value] || value || "-";
}

function buildHeatmapDates(year) {
  const start = new Date(year, 0, 1);
  start.setDate(start.getDate() - start.getDay());
  const end = new Date(year, 11, 31);
  end.setDate(end.getDate() + (6 - end.getDay()));
  const dates = [];
  for (const cursor = new Date(start); cursor <= end; cursor.setDate(cursor.getDate() + 1)) {
    dates.push(new Date(cursor));
  }
  return dates;
}

function heatLevel(count, maxCount) {
  if (count <= 0 || maxCount <= 0) {
    return 0;
  }
  return Math.max(1, Math.min(4, Math.ceil((Math.log1p(count) / Math.log1p(maxCount)) * 4)));
}

function formatDateKey(date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function parseDateKey(value) {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(String(value || ""));
  return match ? new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3])) : null;
}

function isValidLayout(value) {
  return /^[1-7]{7}$/.test(value) && new Set(value).size === 7;
}

function clearDayDetails() {
  state.selectedDate = "";
  selectedDateTitle.textContent = currentLabels().selectDate;
  selectedDateSummary.textContent = "";
  historyRows.replaceChildren();
  historyEmpty.textContent = currentLabels().selectDateHelp;
  historyEmpty.classList.remove("hidden");
  renderHeatmap();
}

function currentLabels() {
  return labels[state.language] || labels.ja;
}

function setBusy(value) {
  state.busy = value;
  refreshButton.disabled = value;
  if (value && !state.data) {
    summaryElement.textContent = currentLabels().loading;
  }
}

function showError(message) {
  errorElement.textContent = message;
  errorElement.classList.remove("hidden");
}

function clearError() {
  errorElement.textContent = "";
  errorElement.classList.add("hidden");
}
