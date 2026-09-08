import { Info } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import packageInfo from "../../../../package.json";

const APP_VERSION = packageInfo.version || "0.0.0";

/// 展示个人版定位和当前构建版本；该卡片不读取平台、钱包或模型路由状态。
export function AboutCodexManagerCard({
  t,
}: {
  t: (value: string) => string;
}) {
  return (
    <Card className="glass-card mission-panel shadow-sm">
      <CardHeader>
        <div className="flex items-center gap-2">
          <Info className="h-4 w-4 text-primary" />
          <CardTitle className="text-base">{t("关于 CodexManager")}</CardTitle>
        </div>
        <CardDescription>{t("个人 Codex 账号管理与会话分流工具")}</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="flex flex-wrap items-center gap-3 rounded-lg border border-border/60 bg-background/35 px-4 py-3">
          <span className="font-semibold">CodexManager</span>
          <Badge variant="secondary" className="font-mono">
            v{APP_VERSION}
          </Badge>
          <span className="text-sm text-muted-foreground">
            {t("账号和会话分流由 CM 处理，会话、项目、模型、Skills 与插件继续由 Codex 管理。")}
          </span>
        </div>
      </CardContent>
    </Card>
  );
}
