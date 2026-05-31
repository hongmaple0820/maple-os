"use client";

import { useState } from "react";
import { Button, Badge } from "@mapleos/ui";
import { mapleApi, setAuthState } from "@/lib/api";

interface AuthPageProps {
  onAuth: () => void;
}

export function AuthPage({ onAuth }: AuthPageProps) {
  const [mode, setMode] = useState<"login" | "register">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setError("");
    setLoading(true);

    try {
type AuthResponse = { token?: string; error?: string; user_id?: string; username?: string; role?: string; refresh_token?: string };

      if (mode === "register") {
        const data = await mapleApi<AuthResponse>(
          "/api/auth/register",
          { method: "POST", body: { username, password, email: email || undefined } }
        );
        if (data.error) {
          setError(data.error);
          return;
        }
        if (data.token) {
          setAuthState(data.token, "", {
            user_id: data.user_id ?? "",
            username: data.username ?? username,
            role: data.role ?? "user",
          });
          onAuth();
          return;
        }
      }

      const data = await mapleApi<AuthResponse>(
        "/api/auth/login",
        { method: "POST", body: { username, password } }
      );
      if (data.error) {
        setError(data.error);
        return;
      }
      if (data.token) {
        setAuthState(
          data.token,
          data.refresh_token ?? "",
          {
            user_id: data.user_id ?? "",
            username: data.username ?? username,
            role: data.role ?? "user",
          }
        );
        onAuth();
      }
    } catch (err) {
      setError(mode === "login" ? "Login failed, check username and password" : "Registration failed");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex items-center justify-center h-screen bg-background">
      <div className="w-full max-w-sm space-y-6 p-8 border rounded-xl shadow-lg bg-card">
        <div className="text-center space-y-2">
          <svg className="w-10 h-10 mx-auto text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5" />
          </svg>
          <h1 className="text-xl font-bold">MapleOS</h1>
          <p className="text-sm text-muted-foreground">Agent Operating System</p>
        </div>

        <div className="flex border rounded-md">
          <button
            onClick={() => { setMode("login"); setError(""); }}
            className={`flex-1 py-1.5 text-sm font-medium rounded-l-md transition-colors ${
              mode === "login" ? "bg-primary text-primary-foreground" : "hover:bg-accent"
            }`}
          >
            Login
          </button>
          <button
            onClick={() => { setMode("register"); setError(""); }}
            className={`flex-1 py-1.5 text-sm font-medium rounded-r-md transition-colors ${
              mode === "register" ? "bg-primary text-primary-foreground" : "hover:bg-accent"
            }`}
          >
            Register
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">Username</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full px-3 py-2 text-sm border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary"
              required
              autoFocus
            />
          </div>

          {mode === "register" && (
            <div>
              <label className="block text-xs font-medium text-muted-foreground mb-1">Email (Optional)</label>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="w-full px-3 py-2 text-sm border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary"
              />
            </div>
          )}

          <div>
            <label className="block text-xs font-medium text-muted-foreground mb-1">Password</label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="w-full px-3 py-2 text-sm border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary"
              required
              minLength={mode === "register" ? 6 : undefined}
            />
            {mode === "register" && (
              <p className="text-[11px] text-muted-foreground mt-1">Min 6 characters</p>
            )}
          </div>

          {error && (
            <div className="text-sm text-destructive bg-destructive/10 rounded-md px-3 py-2">
              {error}
            </div>
          )}

          <Button type="submit" className="w-full" disabled={loading}>
            {loading ? "Processing..." : mode === "login" ? "Login" : "Register"}
          </Button>
        </form>

        <div className="text-center text-[11px] text-muted-foreground">
          {mode === "login" ? "Don't have an account? " : "Already have an account? "}
          <button
            type="button"
            className="text-primary hover:underline"
            onClick={() => setMode(mode === "login" ? "register" : "login")}
          >
            {mode === "login" ? "Register" : "Login"}
          </button>
        </div>
      </div>
    </div>
  );
}
