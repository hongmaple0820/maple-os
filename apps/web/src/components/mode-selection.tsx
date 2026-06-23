"use client";

import { useTranslation } from "react-i18next";
import { Button, Badge } from "@mapleos/ui";

interface ModeSelectionProps {
  onSelectLocal: () => void;
  onSelectCloud: () => void;
}

export function ModeSelection({ onSelectLocal, onSelectCloud }: ModeSelectionProps) {
  const { t } = useTranslation();

  return (
    <div className="flex items-center justify-center h-screen bg-background">
      <div className="w-full max-w-lg space-y-8 p-8">
        <div className="text-center space-y-3">
          <svg className="w-12 h-12 mx-auto text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
          <h1 className="text-2xl font-bold">{t("common.appName")}</h1>
          <p className="text-muted-foreground">{t("mode.selectTitle")}</p>
        </div>

        <div className="grid grid-cols-2 gap-4">
          {/* Local Mode */}
          <button
            onClick={onSelectLocal}
            className="group relative rounded-xl border-2 p-6 text-left transition-all hover:border-primary hover:shadow-lg hover:bg-primary/5"
          >
            <div className="space-y-3">
              <div className="w-10 h-10 rounded-lg bg-primary/10 flex items-center justify-center">
                <svg className="w-5 h-5 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M5 12.55a11 11 0 0 1 14.08 0M1.42 9a16 16 0 0 1 21.16 0M8.53 16.11a6 6 0 0 1 6.95 0M12 20h.01" />
                </svg>
              </div>
              <h3 className="text-lg font-semibold">{t("mode.local")}</h3>
              <p className="text-sm text-muted-foreground">{t("mode.localDesc")}</p>
              <div className="flex flex-wrap gap-1.5 pt-2">
                <Badge variant="secondary" className="text-[10px]">{t("mode.lanServer")}</Badge>
                <Badge variant="secondary" className="text-[10px]">{t("mode.deviceId")}</Badge>
                <Badge variant="secondary" className="text-[10px]">{t("mode.tempAccount")}</Badge>
              </div>
            </div>
            <div className="absolute top-4 right-4 opacity-0 group-hover:opacity-100 transition-opacity">
              <svg className="w-5 h-5 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M5 12h14M12 5l7 7-7 7" />
              </svg>
            </div>
          </button>

          {/* Cloud Mode */}
          <button
            onClick={onSelectCloud}
            className="group relative rounded-xl border-2 p-6 text-left transition-all opacity-60 cursor-not-allowed"
            disabled
          >
            <div className="space-y-3">
              <div className="w-10 h-10 rounded-lg bg-muted flex items-center justify-center">
                <svg className="w-5 h-5 text-muted-foreground" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M18 10h-1.26A8 8 0 1 0 9 20h9a5 5 0 0 0 0-10z" />
                </svg>
              </div>
              <h3 className="text-lg font-semibold">{t("mode.cloud")}</h3>
              <p className="text-sm text-muted-foreground">{t("mode.cloudDesc")}</p>
              <Badge variant="outline" className="text-[10px]">{t("mode.comingSoon")}</Badge>
            </div>
          </button>
        </div>
      </div>
    </div>
  );
}
