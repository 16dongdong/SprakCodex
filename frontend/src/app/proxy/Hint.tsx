// 通用「说明」悬停提示:一个小 i 图标,鼠标悬停/聚焦时弹出说明,不在页面上常占位置。
export function Hint({ text }: { text: string }) {
  return (
    <span className="hint" tabIndex={0}>
      i
      <span className="hint-pop">{text}</span>
    </span>
  );
}
