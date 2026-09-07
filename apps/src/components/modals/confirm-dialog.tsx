"use client";

import { useState } from "react";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button, buttonVariants } from "@/components/ui/button";
import { useI18n } from "@/lib/i18n/provider";

interface ConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description: string;
  confirmText?: string;
  cancelText?: string;
  confirmVariant?: "default" | "destructive";
  onConfirm: () => boolean | void | Promise<boolean | void>;
}

/**
 * 函数 `ConfirmDialog`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
export function ConfirmDialog({
  open,
  onOpenChange,
  title,
  description,
  confirmText,
  cancelText,
  confirmVariant = "default",
  onConfirm,
}: ConfirmDialogProps) {
  const { t } = useI18n();
  const [isConfirming, setIsConfirming] = useState(false);

  const handleConfirm = async () => {
    if (isConfirming) return;
    setIsConfirming(true);
    try {
      const shouldClose = await onConfirm();
      if (shouldClose !== false) onOpenChange(false);
    } catch (error) {
      console.error("confirm action failed", error);
    } finally {
      setIsConfirming(false);
    }
  };

  const handleOpenChange = (nextOpen: boolean) => {
    if (isConfirming && !nextOpen) return;
    onOpenChange(nextOpen);
  };

  return (
    <Dialog
      open={open}
      onOpenChange={handleOpenChange}
      disablePointerDismissal={isConfirming}
    >
      <DialogContent
        showCloseButton={false}
        className="glass-card p-6 sm:max-w-[420px]"
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
          <DialogDescription className="break-words">
            {description}
          </DialogDescription>
        </DialogHeader>

        <DialogFooter className="gap-2 sm:gap-2">
          <DialogClose
            className={buttonVariants({ variant: "outline" })}
            type="button"
            disabled={isConfirming}
          >
            {cancelText || t("取消")}
          </DialogClose>
          <Button
            variant={confirmVariant}
            disabled={isConfirming}
            onClick={() => void handleConfirm()}
          >
            {confirmText || t("确定")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
