"use client";

import {
  ChevronDown,
  Eye,
  EyeOff,
  Loader2,
  PowerOff,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useI18n } from "@/lib/i18n/provider";
import type { ManagedModelV2, ModelVisibilityV2 } from "@/types/model-v2";

type ModelOperationalState =
  | "visible_enabled"
  | "visible_disabled"
  | "hidden_enabled"
  | "hidden_disabled";

export type ModelStateTarget = {
  enabled: boolean;
  visibility: ModelVisibilityV2;
};

export const MODEL_OPERATIONAL_STATE_TARGETS: Record<
  ModelOperationalState,
  ModelStateTarget
> = {
  visible_enabled: { enabled: true, visibility: "list" },
  visible_disabled: { enabled: false, visibility: "list" },
  hidden_enabled: { enabled: true, visibility: "hide" },
  hidden_disabled: { enabled: false, visibility: "hide" },
};

function modelOperationalState(model: ManagedModelV2): ModelOperationalState {
  if (model.visibility === "hide") {
    return model.enabled ? "hidden_enabled" : "hidden_disabled";
  }
  return model.enabled ? "visible_enabled" : "visible_disabled";
}

export function ModelStateDropdown({
  model,
  disabled,
  isUpdating,
  onStateChange,
}: {
  model: ManagedModelV2;
  disabled: boolean;
  isUpdating: boolean;
  onStateChange: (target: ModelStateTarget) => void;
}) {
  const { t } = useI18n();
  const state = modelOperationalState(model);
  const isHidden = model.visibility === "hide";
  const label = isHidden
    ? model.enabled
      ? t("隐藏且启用")
      : t("隐藏且禁用")
    : model.enabled
      ? t("已启用")
      : t("已禁用");
  const StateIcon = isHidden ? EyeOff : model.enabled ? Eye : PowerOff;

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        disabled={disabled}
        aria-label={t("模型状态操作 {slug}", { slug: model.slug })}
        render={
          <Button
            variant="outline"
            size="sm"
            className="min-w-[104px] justify-between"
            disabled={disabled}
            render={<span />}
            nativeButton={false}
          />
        }
        nativeButton={false}
      >
        {isUpdating ? (
          <Loader2 className="h-3.5 w-3.5 animate-spin" />
        ) : (
          <StateIcon className="h-3.5 w-3.5" />
        )}
        <span>{label}</span>
        <ChevronDown className="h-3.5 w-3.5 text-muted-foreground" />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-48">
        <DropdownMenuGroup>
          <DropdownMenuLabel>{t("模型状态")}</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={state}
            disabled={disabled}
            onValueChange={(value) => {
              const nextState = value as ModelOperationalState;
              if (nextState !== state) {
                onStateChange(MODEL_OPERATIONAL_STATE_TARGETS[nextState]);
              }
            }}
          >
            <DropdownMenuRadioItem value="visible_enabled" closeOnClick>
              <Eye className="h-4 w-4" />
              {isHidden ? t("恢复并启用") : t("显示并启用")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="visible_disabled" closeOnClick>
              <PowerOff className="h-4 w-4" />
              {isHidden ? t("恢复显示但保持禁用") : t("显示但禁用")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="hidden_enabled" closeOnClick>
              <EyeOff className="h-4 w-4" />
              {t("隐藏但启用")}
            </DropdownMenuRadioItem>
            <DropdownMenuRadioItem value="hidden_disabled" closeOnClick>
              <EyeOff className="h-4 w-4" />
              {t("隐藏并禁用")}
            </DropdownMenuRadioItem>
          </DropdownMenuRadioGroup>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
