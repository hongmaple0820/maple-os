import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "枫信工作站 - MapleOS",
  description: "AI 原生协作工作站操作系统，人机协同新时代",
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body className="min-h-screen bg-background font-sans antialiased">
        {children}
      </body>
    </html>
  );
}