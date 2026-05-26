"use client";

import React from "react";
import KanbanBoard from "@/components/collaboration/kanban-board";
import OnlineStatus from "@/components/collaboration/online-status";
import Comments from "@/components/collaboration/comments";

export default function CollaborationDemo() {
  return (
    <div className="min-h-screen bg-background p-6">
      <div className="max-w-7xl mx-auto space-y-6">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-bold">协作工作空间</h1>
            <p className="text-muted-foreground mt-1">团队协作面板演示</p>
          </div>
        </div>

        <div className="grid grid-cols-12 gap-6">
          {/* Kanban Board - takes 8 columns */}
          <div className="col-span-8">
            <KanbanBoard
              onTaskMove={(taskId, source, target) => {
                console.log(`Task ${taskId} moved from ${source} to ${target}`);
              }}
              onTaskClick={(task) => {
                console.log("Task clicked:", task);
              }}
            />
          </div>

          {/* Sidebar - takes 4 columns */}
          <div className="col-span-4 space-y-6">
            <OnlineStatus />
            <Comments
              onSendComment={(content) => console.log("New comment:", content)}
              onReply={(commentId, content) => console.log("Reply to", commentId, ":", content)}
              onEdit={(commentId, content) => console.log("Edit comment", commentId, ":", content)}
              onDelete={(commentId) => console.log("Delete comment:", commentId)}
              onLike={(commentId) => console.log("Like comment:", commentId)}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
