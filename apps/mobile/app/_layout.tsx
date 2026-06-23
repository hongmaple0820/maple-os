import { Stack, useRouter, useSegments } from "expo-router";
import { useEffect, useState } from "react";
import { isMobileAuthenticated } from "../src/lib/api";

export default function RootLayout() {
  const router = useRouter();
  const segments = useSegments();
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const checkAuth = async () => {
      const authenticated = await isMobileAuthenticated();
      const inLoginScreen = segments[0] === "login";

      if (!authenticated && !inLoginScreen) {
        router.replace("/login");
      } else if (authenticated && inLoginScreen) {
        router.replace("/(tabs)/dashboard");
      }
      setReady(true);
    };
    checkAuth();
  }, [segments]);

  if (!ready) return null;

  return (
    <Stack
      screenOptions={{
        headerStyle: { backgroundColor: "#1a1a2e" },
        headerTintColor: "#e0e0e0",
        headerTitleStyle: { fontWeight: "600" },
        contentStyle: { backgroundColor: "#0f0f23" },
      }}
    >
      <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
      <Stack.Screen name="login" options={{ headerShown: false }} />
    </Stack>
  );
}