"use client";

import {
  ChevronDown,
  Eye,
  EyeOff,
  ListChecks,
  Loader2,
  PowerOff,
} from "lucide-react";

import {
  MODEL_OPERATIONAL_STATE_TARGETS,
  type ModelStateTarget,
} from "@/components/models/model-state-dropdown";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useI18n } from "@/lib/i18n/provider";

export function BatchModelStateDropdown({
  selectedCount,
  disabled,
  isUpdating,
  onStateChange,
}: {
  selectedCount: number;
  disabled: boolean;
  isUpdating: boolean;
  onStateChange: (target: ModelStateTarget) => void | Promise<void>;
}) {
  const { t } = useI18n();

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={disabled}
        aria-label={t("批量修改模型状态 ({count})", {
          count: selectedCount,
        })}
        render={
          <Button
            size="sm"
            variant="outline"
            disabled={disabled}
            render={<span />}
            nativeButton={false}
          />
        }
        nativeButton={false}
      >
        {isUpdating ? (
          <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
        ) : (
          <ListChecks className="mr-1.5 h-4 w-4" />
        )}
        {t("批量修改状态")} ({selectedCount})
        <ChevronDown className="ml-1.5 h-3.5 w-3.5 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("设置选中模型状态")}</DropdownMenuLabel>
          <DropdownMenuItem
            closeOnClick
            onClick={() =>
              void onStateChange(
                MODEL_OPERATIONAL_STATE_TARGETS.visible_enabled,
              )
            }
          >
            <Eye className="h-4 w-4" />
            {t("显示并启用")}
          </DropdownMenuItem>
          <DropdownMenuItem
            closeOnClick
            onClick={() =>
              void onStateChange(
                MODEL_OPERATIONAL_STATE_TARGETS.visible_disabled,
              )
            }
          >
            <PowerOff className="h-4 w-4" />
            {t("显示但禁用")}
          </DropdownMenuItem>
          <DropdownMenuItem
            closeOnClick
            onClick={() =>
              void onStateChange(
                MODEL_OPERATIONAL_STATE_TARGETS.hidden_enabled,
              )
            }
          >
            <EyeOff className="h-4 w-4" />
            {t("隐藏但启用")}
          </DropdownMenuItem>
          <DropdownMenuItem
            closeOnClick
            onClick={() =>
              void onStateChange(
                MODEL_OPERATIONAL_STATE_TARGETS.hidden_disabled,
              )
            }
          >
            <EyeOff className="h-4 w-4" />
            {t("隐藏并禁用")}
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
