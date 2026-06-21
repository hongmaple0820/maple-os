"use client";

import { Card, CardContent, Spinner } from "@mapleos/ui";
import { useTranslation } from "react-i18next";

type StateStatus = "loading" | "empty" | "error" | "success" | "disabled";

export interface StatePanelProps {
  status: StateStatus;
  title?: string;
  message?: string;
  /** Link to issue or roadmap for disabled/mock features */
  hint?: string;
  children?: React.ReactNode;
}

/**
 * #93: Shared state panel for loading / empty / error / success / disabled.
 * Every module should use this instead of ad-hoc state rendering.
 */
export function StatePanel({ status, title, message, hint, children }: StatePanelProps) {
  const { t } = useTranslation();

  if (status === "loading") {
    return (
      <Card>
        <CardContent className="flex items-center gap-2 py-3 text-sm text-muted-foreground">
          <Spinner className="h-4 w-4" />
          {title ?? t("common.loading", "Loading...")}
        </CardContent>
      </Card>
    );
  }

  if (status === "error") {
    return (
      <Card>
        <CardContent className="py-3 text-sm text-destructive">
          <div className="font-medium">{title ?? t("common.error", "Error")}</div>
          {message && <div className="mt-1 text-xs">{message}</div>}
        </CardContent>
      </Card>
    );
  }

  if (status === "empty") {
    return (
      <Card>
        <CardContent className="py-8 text-center text-sm text-muted-foreground">
          {message ?? t("common.noData", "No data")}
        </CardContent>
      </Card>
    );
  }

  if (status === "disabled") {
    return (
      <Card>
        <CardContent className="py-3 text-sm text-muted-foreground">
          <div className="flex items-center gap-2">
            <Badge variant="outline" className="text-[10px] opacity-60">MOCK</Badge>
            <span>{message ?? t("common.mockFeature", "This feature is not yet available")}</span>
          </div>
          {hint && <div className="mt-1 text-xs text-blue-500 hover:underline cursor-pointer">{hint}</div>}
        </CardContent>
      </Card>
    );
  }

  // success — render children
  return <>{children}</>;
}

// Import Badge lazily to avoid circular deps
import { Badge } from "@mapleos/ui";
