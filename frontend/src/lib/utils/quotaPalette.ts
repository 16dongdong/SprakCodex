// 额度颜色只由剩余比例决定；缺失或非法值使用灰色，边界 30 和 60 均属于黄色。
export function quotaPalette(value: number | null | undefined) {
  if (value == null || !Number.isFinite(value) || value < 0) return { track: "bg-zinc-200 dark:bg-zinc-700", indicator: "bg-zinc-400" };
  if (value > 60) return { track: "bg-green-500/15", indicator: "bg-green-500" };
  if (value >= 30) return { track: "bg-yellow-500/15", indicator: "bg-yellow-500" };
  return { track: "bg-red-500/15", indicator: "bg-red-500" };
}
