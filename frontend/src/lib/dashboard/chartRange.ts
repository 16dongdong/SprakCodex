export type ChartPeriod = "month" | "week" | "day";

// 用本地日历运算生成左闭右开区间；周一为周起点，避免按固定秒数计算造成夏令时偏移。
export function getChartRange(period: ChartPeriod, dayStartTs: number) {
  const start = new Date(dayStartTs * 1000);
  const end = new Date(start);
  if (period === "month") {
    start.setDate(1);
    end.setMonth(start.getMonth() + 1, 1);
  } else if (period === "week") {
    start.setDate(start.getDate() - (start.getDay() + 6) % 7);
    end.setTime(start.getTime());
    end.setDate(end.getDate() + 7);
  } else {
    end.setDate(end.getDate() + 1);
  }
  return { startTs: Math.floor(start.getTime() / 1000), endTs: Math.floor(end.getTime() / 1000) };
}
