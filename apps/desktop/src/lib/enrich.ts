import type { Location, Weather } from "../types";

const WMO: Record<number, string> = {
  0: "Clear", 1: "Mostly clear", 2: "Partly cloudy", 3: "Overcast",
  45: "Fog", 48: "Rime fog", 51: "Light drizzle", 53: "Drizzle", 55: "Heavy drizzle",
  61: "Light rain", 63: "Rain", 65: "Heavy rain", 66: "Freezing rain", 67: "Freezing rain",
  71: "Light snow", 73: "Snow", 75: "Heavy snow", 77: "Snow grains",
  80: "Showers", 81: "Showers", 82: "Violent showers", 85: "Snow showers", 86: "Snow showers",
  95: "Thunderstorm", 96: "Thunderstorm with hail", 99: "Thunderstorm with hail",
};

export function captureEnabled(): boolean {
  try {
    return localStorage.getItem("journal.capture") !== "off";
  } catch {
    return true;
  }
}

export function setCaptureEnabled(on: boolean) {
  try {
    localStorage.setItem("journal.capture", on ? "on" : "off");
  } catch {
    /* ignore */
  }
}

function geolocate(timeoutMs = 6000): Promise<Location | null> {
  return new Promise((resolve) => {
    if (!("geolocation" in navigator)) return resolve(null);
    const t = setTimeout(() => resolve(null), timeoutMs);
    navigator.geolocation.getCurrentPosition(
      (p) => {
        clearTimeout(t);
        resolve({ latitude: +p.coords.latitude.toFixed(5), longitude: +p.coords.longitude.toFixed(5) });
      },
      () => {
        clearTimeout(t);
        resolve(null);
      },
      { maximumAge: 10 * 60 * 1000, timeout: timeoutMs },
    );
  });
}

async function weatherAt(loc: Location): Promise<Weather | null> {
  try {
    const url = `https://api.open-meteo.com/v1/forecast?latitude=${loc.latitude}&longitude=${loc.longitude}&current=temperature_2m,weather_code`;
    const r = await fetch(url);
    if (!r.ok) return null;
    const j = (await r.json()) as { current?: { temperature_2m?: number; weather_code?: number } };
    const c = j.current;
    if (!c) return null;
    return { temperature_c: c.temperature_2m ?? null, conditions: c.weather_code != null ? WMO[c.weather_code] ?? null : null };
  } catch {
    return null;
  }
}

/** Approximate position from the network connection (city level). Used when the
 *  OS location service is unavailable, which is the case inside the macOS webview. */
async function networkLocate(): Promise<Location | null> {
  try {
    const r = await fetch("https://ipapi.co/json/", { headers: { Accept: "application/json" } });
    if (!r.ok) return null;
    const j = (await r.json()) as { latitude?: number; longitude?: number; city?: string; region?: string };
    if (typeof j.latitude !== "number" || typeof j.longitude !== "number") return null;
    const place = [j.city, j.region].filter(Boolean).join(", ");
    return { latitude: +j.latitude.toFixed(3), longitude: +j.longitude.toFixed(3), place_name: place || null };
  } catch {
    return null;
  }
}

/** Best effort; resolves to whatever could be captured, never throws. */
export async function captureContext(): Promise<{ location?: Location; weather?: Weather }> {
  if (!captureEnabled()) return {};
  const location = (await geolocate()) ?? (await networkLocate());
  if (!location) return {};
  const weather = await weatherAt(location);
  return weather ? { location, weather } : { location };
}
