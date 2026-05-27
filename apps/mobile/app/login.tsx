import { View, Text, TextInput, TouchableOpacity, StyleSheet, Alert } from "react-native";
import { useState } from "react";
import { useRouter } from "expo-router";
import { setMobileAuthState, BASE_URL } from "../src/lib/api";

export default function LoginScreen() {
  const router = useRouter();
  const [mode, setMode] = useState<"login" | "register">("login");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [email, setEmail] = useState("");
  const [loading, setLoading] = useState(false);

  const handleSubmit = async () => {
    if (!username.trim() || !password.trim()) {
      Alert.alert("Error", "Username and password are required");
      return;
    }
    if (password.length < 6) {
      Alert.alert("Error", "Password must be at least 6 characters");
      return;
    }

    setLoading(true);
    try {
      const endpoint = mode === "login" ? "/api/auth/login" : "/api/auth/register";
      const body: Record<string, string> = { username: username.trim(), password };
      if (mode === "register" && email.trim()) body.email = email.trim();

      const res = await fetch(`${BASE_URL}${endpoint}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      const data = await res.json();
      if (!res.ok) {
        Alert.alert("Error", data.error || "Authentication failed");
        return;
      }

      if (data.token) {
        await setMobileAuthState(data.token, data.refresh_token, {
          user_id: data.user_id,
          username: data.username,
          role: data.role,
        });
        router.replace("/(tabs)/dashboard");
      }
    } catch (err) {
      Alert.alert("Error", `Network error: ${(err as Error).message}`);
    } finally {
      setLoading(false);
    }
  };

  return (
    <View style={styles.container}>
      <Text style={styles.title}>MapleOS</Text>
      <Text style={styles.subtitle}>AI Native Multi-Agent Workstation</Text>

      <View style={styles.form}>
        <TextInput
          style={styles.input}
          placeholder="Username"
          placeholderTextColor="#666"
          value={username}
          onChangeText={setUsername}
          autoCapitalize="none"
        />
        <TextInput
          style={styles.input}
          placeholder="Password"
          placeholderTextColor="#666"
          value={password}
          onChangeText={setPassword}
          secureTextEntry
        />
        {mode === "register" && (
          <TextInput
            style={styles.input}
            placeholder="Email (optional)"
            placeholderTextColor="#666"
            value={email}
            onChangeText={setEmail}
            autoCapitalize="none"
            keyboardType="email-address"
          />
        )}

        <TouchableOpacity
          style={[styles.button, loading && styles.buttonDisabled]}
          onPress={handleSubmit}
          disabled={loading}
        >
          <Text style={styles.buttonText}>{loading ? "Processing..." : mode === "login" ? "Login" : "Register"}</Text>
        </TouchableOpacity>

        <TouchableOpacity onPress={() => setMode(mode === "login" ? "register" : "login")}>
          <Text style={styles.switchText}>
            {mode === "login" ? "Don't have an account? Register" : "Already have an account? Login"}
          </Text>
        </TouchableOpacity>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: "#0f0f23", justifyContent: "center", padding: 24 },
  title: { color: "#e0e0e0", fontSize: 32, fontWeight: "700", textAlign: "center", marginBottom: 4 },
  subtitle: { color: "#888", fontSize: 14, textAlign: "center", marginBottom: 40 },
  form: { gap: 12 },
  input: { backgroundColor: "#1a1a2e", color: "#e0e0e0", borderRadius: 8, padding: 14, fontSize: 15, borderWidth: 1, borderColor: "#2a2a4a" },
  button: { backgroundColor: "#6366f1", borderRadius: 8, padding: 14, alignItems: "center", marginTop: 8 },
  buttonDisabled: { opacity: 0.6 },
  buttonText: { color: "#fff", fontSize: 16, fontWeight: "600" },
  switchText: { color: "#6366f1", fontSize: 13, textAlign: "center", marginTop: 16 },
});
