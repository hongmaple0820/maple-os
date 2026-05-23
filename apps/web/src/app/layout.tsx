import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "MapleOS - Agent Collaboration Workstation",
  description: "AI Native workstation operating system for human-agent collaboration",
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