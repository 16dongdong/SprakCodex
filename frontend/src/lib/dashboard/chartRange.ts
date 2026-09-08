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

// 日视图按真实经过的一小时分桶，周/月按本地日历日分桶；包含周期终点供服务端使用半开区间。
export function getChartBoundaries(period: ChartPeriod, dayStartTs: number): number[] {
  const range = getChartRange(period, dayStartTs);
  const boundaries = [range.startTs];
  const cursor = new Date(range.startTs * 1000);
  while (cursor.getTime() / 1000 < range.endTs) {
    if (period === "day") cursor.setTime(cursor.getTime() + 3600_000);
    else cursor.setDate(cursor.getDate() + 1);
    boundaries.push(Math.min(range.endTs, cursor.getTime() / 1000));
  }
  return boundaries;
}
