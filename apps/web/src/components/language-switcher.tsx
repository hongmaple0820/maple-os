"use client";

import { useTranslation } from "react-i18next";
import { useState, useEffect } from "react";

export function LanguageSwitcher() {
  const { i18n } = useTranslation();
  const [mounted, setMounted] = useState(false);

  useEffect(() => { setMounted(true); }, []);

  if (!mounted) return null;

  const currentLang = i18n.language?.startsWith("zh") ? "zh" : "en";

  const toggle = () => {
    const next = currentLang === "zh" ? "en" : "zh";
    i18n.changeLanguage(next);
  };

  return (
    <button
      onClick={toggle}
      className="px-2 py-1 rounded text-[11px] border hover:bg-accent transition-colors font-mono"
      title={currentLang === "zh" ? "Switch to English" : "切换到中文"}
    >
      {currentLang === "zh" ? "EN" : "中"}
    </button>
  );
}
