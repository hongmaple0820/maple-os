"use client";

import React, { useState, useEffect } from "react";
import {
  Users,
  Circle,
  Clock,
  Zap,
  Moon,
  Briefcase,
  MoreHorizontal,
  Phone,
  Video,
  MessageCircle,
  Mail
} from "lucide-react";

export interface UserStatus {
  id: string;
  name: string;
  avatar?: string;
  status: "online" | "away" | "busy" | "offline";
  activity?: string;
  lastSeen?: string;
  role?: string;
  isCurrentUser?: boolean;
}

interface OnlineStatusProps {
  users?: UserStatus[];
  maxDisplay?: number;
  showActivity?: boolean;
}

const defaultUsers: UserStatus[] = [
  {
    id: "user-1",
    name: "我",
    avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=me",
    status: "online",
    activity: "编辑任务看板",
    role: "产品经理",
    isCurrentUser: true,
  },
  {
    id: "user-2",
    name: "张三",
    avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=zhang",
    status: "online",
    activity: "设计系统架构图",
    role: "架构师",
  },
  {
    id: "user-3",
    name: "李四",
    avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=li",
    status: "busy",
    activity: "视频会议中",
    role: "前端开发",
  },
  {
    id: "user-4",
    name: "王五",
    avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=wang",
    status: "online",
    activity: "编写 API 文档",
    role: "后端开发",
  },
  {
    id: "user-5",
    name: "赵六",
    avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=zhao",
    status: "away",
    lastSeen: "10分钟前",
    role: "UI 设计师",
  },
  {
    id: "user-6",
    name: "钱七",
    avatar: "https://api.dicebear.com/7.x/avataaars/svg?seed=qian",
    status: "offline",
    lastSeen: "2小时前",
    role: "测试工程师",
  },
];

const statusConfig = {
  online: { color: "bg-emerald-500", text: "text-emerald-600", label: "在线", icon: Zap },
  away: { color: "bg-amber-500", text: "text-amber-600", label: "离开", icon: Clock },
  busy: { color: "bg-rose-500", text: "text-rose-600", label: "忙碌", icon: Briefcase },
  offline: { color: "bg-slate-400", text: "text-slate-500", label: "离线", icon: Moon },
};

export function OnlineStatus({ users = defaultUsers, maxDisplay = 5, showActivity = true }: OnlineStatusProps) {
  const [onlineCount, setOnlineCount] = useState(0);
  const [currentStatus, setCurrentStatus] = useState<UserStatus["status"]>("online");
  const [isDropdownOpen, setIsDropdownOpen] = useState(false);

  useEffect(() => {
    const online = users.filter((u) => u.status === "online").length;
    setOnlineCount(online);
  }, [users]);

  const currentUser = users.find((u) => u.isCurrentUser);
  const otherUsers = users.filter((u) => !u.isCurrentUser);
  const displayedUsers = otherUsers.slice(0, maxDisplay);
  const remainingCount = otherUsers.length - maxDisplay;

  const handleStatusChange = (status: UserStatus["status"]) => {
    setCurrentStatus(status);
    setIsDropdownOpen(false);
  };

  return (
    <div className="bg-card rounded-xl border border-border shadow-card overflow-hidden">
      <div className="px-6 py-4 border-b border-border">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <h2 className="text-lg font-semibold">团队成员</h2>
            <span className="flex items-center gap-1.5 px-2.5 py-1 bg-emerald-50 rounded-full text-xs font-medium text-emerald-600">
              <span className="w-2 h-2 rounded-full bg-emerald-500 animate-pulse" />
              {onlineCount} 人在线
            </span>
          </div>
          <button className="p-1.5 text-muted-foreground hover:text-foreground rounded-lg hover:bg-muted transition-colors">
            <MoreHorizontal className="w-5 h-5" />
          </button>
        </div>
      </div>

      <div className="p-4">
        {/* Current User Status */}
        {currentUser && (
          <div className="mb-4 p-3 bg-muted/50 rounded-lg">
            <div className="flex items-center gap-3">
              <div className="relative">
                <img
                  src={currentUser.avatar}
                  alt={currentUser.name}
                  className="w-10 h-10 rounded-full bg-muted"
                />
                <span className={`absolute -bottom-0.5 -right-0.5 w-3.5 h-3.5 rounded-full border-2 border-card ${statusConfig[currentStatus].color}`} />
              </div>
              <div className="flex-1">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-sm">{currentUser.name}</span>
                  <span className="text-xs text-muted-foreground">({currentUser.role})</span>
                </div>
                <div className="relative">
                  <button
                    onClick={() => setIsDropdownOpen(!isDropdownOpen)}
                    className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors"
                  >
                    <span className={`w-2 h-2 rounded-full ${statusConfig[currentStatus].color}`} />
                    {statusConfig[currentStatus].label}
                  </button>
                  {isDropdownOpen && (
                    <div className="absolute top-full left-0 mt-1 py-1 bg-card border border-border rounded-lg shadow-lg z-10 min-w-[120px]">
                      {Object.entries(statusConfig).map(([status, config]) => (
                        <button
                          key={status}
                          onClick={() => handleStatusChange(status as UserStatus["status"])}
                          className="w-full flex items-center gap-2 px-3 py-2 text-xs hover:bg-muted transition-colors"
                        >
                          <span className={`w-2 h-2 rounded-full ${config.color}`} />
                          <span>{config.label}</span>
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Team Members */}
        <div className="space-y-1">
          {displayedUsers.map((user) => (
            <div
              key={user.id}
              className="flex items-center gap-3 p-2.5 rounded-lg hover:bg-muted/50 transition-colors cursor-pointer group"
            >
              <div className="relative">
                <img
                  src={user.avatar}
                  alt={user.name}
                  className="w-9 h-9 rounded-full bg-muted"
                />
                <span
                  className={`absolute -bottom-0.5 -right-0.5 w-3 h-3 rounded-full border-2 border-card ${statusConfig[user.status].color} ${
                    user.status === "online" ? "animate-pulse" : ""
                  }`}
                />
              </div>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-medium text-sm truncate">{user.name}</span>
                  <span className="text-xs text-muted-foreground shrink-0">({user.role})</span>
                </div>
                {showActivity && (
                  <div className="flex items-center gap-1.5 text-xs text-muted-foreground">
                    {user.activity ? (
                      <>
                        <span className="w-1.5 h-1.5 rounded-full bg-primary" />
                        <span className="truncate">{user.activity}</span>
                      </>
                    ) : (
                      <span>上次活跃: {user.lastSeen}</span>
                    )}
                  </div>
                )}
              </div>
              <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                <button className="p-1.5 text-muted-foreground hover:text-primary hover:bg-primary/10 rounded transition-colors">
                  <MessageCircle className="w-4 h-4" />
                </button>
                <button className="p-1.5 text-muted-foreground hover:text-primary hover:bg-primary/10 rounded transition-colors">
                  <Video className="w-4 h-4" />
                </button>
              </div>
            </div>
          ))}
        </div>

        {/* More Users */}
        {remainingCount > 0 && (
          <button className="w-full mt-2 py-2 flex items-center justify-center gap-2 text-sm text-muted-foreground hover:text-foreground hover:bg-muted rounded-lg transition-colors">
            <Users className="w-4 h-4" />
            还有 {remainingCount} 位成员
          </button>
        )}
      </div>

      {/* Quick Actions */}
      <div className="px-4 py-3 border-t border-border bg-muted/30">
        <div className="flex items-center gap-2">
          <button className="flex-1 flex items-center justify-center gap-1.5 py-2 px-3 bg-primary text-primary-foreground rounded-lg text-sm font-medium hover:opacity-90 transition-opacity">
            <Users className="w-4 h-4" />
            邀请成员
          </button>
          <button className="flex-1 flex items-center justify-center gap-1.5 py-2 px-3 border border-border rounded-lg text-sm font-medium hover:bg-muted transition-colors">
            <Video className="w-4 h-4" />
            视频会议
          </button>
        </div>
      </div>
    </div>
  );
}

export default OnlineStatus;
