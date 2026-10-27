// 設定画面の入力の解釈（DOM に依存しない）

export const MAX_REMINDER_MINUTES = 1440;

/** "10, 1" → [10, 1]。空なら []。数字以外・範囲外が混ざっていたら null */
export function parseMinutesList(text: string): number[] | null {
  const parts = text
    .split(/[,、，\s]+/)
    .map((p) => p.trim())
    .filter((p) => p !== "");
  const result: number[] = [];
  for (const p of parts) {
    if (!/^\d+$/.test(p)) return null;
    const n = Number(p);
    if (n > MAX_REMINDER_MINUTES) return null;
    if (!result.includes(n)) result.push(n);
  }
  return result.sort((a, b) => b - a);
}

/** "Call, Meeting" → ["Call", "Meeting"]（空の要素と重複は除く） */
export function parseTypes(text: string): string[] {
  const result: string[] = [];
  for (const p of text.split(/[,、，]/)) {
    const t = p.trim();
    if (t && !result.includes(t)) result.push(t);
  }
  return result;
}
