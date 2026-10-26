// Applies Feast's native settings to the generated iOS and Android projects.
// Safe to run again (it rewrites the Feast-managed parts only), so after
// `npx cap add ...` or after editing shared/countries.json / shared/apps.json,
// run `npm run build` (which runs this) and commit the result.
//
// Android: permissions, deep links (custom scheme + verified https app links
// per country host), notification icon/colour/channel, ML Kit scanner module,
// MainActivity that exposes the native bridge to the live Feast sites.
// iOS: camera text, URL scheme, push background mode, entitlements (push +
// associated domains), deployment target 15.5, notification forwarding.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const shared = dirname(fileURLToPath(import.meta.url));
const root = dirname(shared);
const apps = JSON.parse(readFileSync(join(shared, "apps.json"), "utf8"));
const { countries } = JSON.parse(readFileSync(join(shared, "countries.json"), "utf8"));
const hosts = countries.map((c) => new URL(c.url).host);

/** Paths each app claims as https app links / universal links. */
const LINK_PATHS = {
  diner: ["/r/", "/t/", "/restaurants/", "/bookings", "/pay/", "/payments/", "/dine-in", "/account"],
  partner: ["/partner"],
};

// Core + Feast plugins whose JS is exposed to the live site on Android.
const BRIDGE_PLUGINS = [
  "CapacitorCookies",
  "CapacitorHttp",
  "WebView",
  "SystemBars",
  "App",
  "Browser",
  "Preferences",
  "SplashScreen",
  "StatusBar",
  "FirebaseMessaging",
  "BarcodeScanner",
];

const BEGIN = "<!-- feast:begin -->";
const END = "<!-- feast:end -->";

function read(p) {
  return readFileSync(p, "utf8");
}
function write(p, s) {
  mkdirSync(dirname(p), { recursive: true });
  writeFileSync(p, s);
}
/** Replace (or insert just before the line holding `anchor`) a block between markers, indented one level deeper than the anchor. */
function upsertBlock(text, block, anchor, begin = BEGIN, end = END) {
  text = text.replace(new RegExp(`\\n[ \\t]*${escape(begin)}[\\s\\S]*?${escape(end)}`), "");
  const i = text.indexOf(anchor);
  if (i < 0) throw new Error(`anchor not found: ${anchor}`);
  const lineStart = text.lastIndexOf("\n", i) + 1;
  const inner = text.slice(lineStart, i) + "    ";
  const lines = [begin, ...block.split("\n"), end].map((l) => (l ? inner + l : l)).join("\n");
  return text.slice(0, lineStart) + lines + "\n" + text.slice(lineStart);
}
function escape(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
function ensureLine(file, line) {
  const t = existsSync(file) ? read(file) : "";
  if (!t.split("\n").includes(line)) write(file, (t.endsWith("\n") || !t ? t : t + "\n") + line + "\n");
}

for (const [name, app] of Object.entries(apps)) {
  if (name.startsWith("_")) continue;
  const dir = join(root, name);
  if (existsSync(join(dir, "android"))) android(name, app, dir);
  if (existsSync(join(dir, "ios"))) ios(name, app, dir);
}

function android(name, app, dir) {
  const a = join(dir, "android");
  const main = join(a, "app", "src", "main");

  // --- AndroidManifest.xml
  const mf = join(main, "AndroidManifest.xml");
  let m = read(mf);
  const linkFilters = hosts.length
    ? `<intent-filter android:autoVerify="true">
    <action android:name="android.intent.action.VIEW" />
    <category android:name="android.intent.category.DEFAULT" />
    <category android:name="android.intent.category.BROWSABLE" />
    <data android:scheme="https" />
${hosts.map((h) => `    <data android:host="${h}" />`).join("\n")}
${LINK_PATHS[name].map((p) => `    <data android:pathPrefix="${p}" />`).join("\n")}
</intent-filter>`
    : "";
  m = upsertBlock(
    m,
    `<!-- Feast links: ${app.scheme}://… and https://<country site>/… open in the app -->
<intent-filter>
    <action android:name="android.intent.action.VIEW" />
    <category android:name="android.intent.category.DEFAULT" />
    <category android:name="android.intent.category.BROWSABLE" />
    <data android:scheme="${app.scheme}" />
</intent-filter>
${linkFilters}`,
    "</activity>",
  );
  m = upsertBlock(
    m,
    `<!-- Push notifications (Firebase Cloud Messaging) -->
<meta-data android:name="com.google.firebase.messaging.default_notification_icon" android:resource="@drawable/ic_stat_feast" />
<meta-data android:name="com.google.firebase.messaging.default_notification_color" android:resource="@color/feast_brand" />
<meta-data android:name="com.google.firebase.messaging.default_notification_channel_id" android:value="feast_default" />
<!-- QR scanning uses Google's code scanner (no camera permission needed) -->
<meta-data android:name="com.google.mlkit.vision.DEPENDENCIES" android:value="barcode_ui" />`,
    "</application>",
    "<!-- feast:meta:begin -->",
    "<!-- feast:meta:end -->",
  );
  m = upsertBlock(
    m,
    `<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />`,
    "</manifest>",
    "<!-- feast:perm:begin -->",
    "<!-- feast:perm:end -->",
  );
  write(mf, m);

  // --- colours + monochrome notification icon
  write(
    join(main, "res", "values", "feast_colors.xml"),
    `<?xml version="1.0" encoding="utf-8"?>
<!-- Generated by mobile/shared/configure-native.mjs -->
<resources>
    <color name="feast_brand">${name === "partner" ? "#0F3D3A" : "#0F766E"}</color>
</resources>
`,
  );
  write(
    join(main, "res", "drawable", "ic_stat_feast.xml"),
    `<?xml version="1.0" encoding="utf-8"?>
<!-- Notification icon: the Feast "f." mark, white on transparent (Android tints it). -->
<vector xmlns:android="http://schemas.android.com/apk/res/android"
    android:width="24dp"
    android:height="24dp"
    android:viewportWidth="1024"
    android:viewportHeight="1024">
    <path
        android:pathData="M616,300 Q616,236 548,236 Q466,236 466,326 L466,780"
        android:strokeWidth="112"
        android:strokeColor="#FFFFFFFF"
        android:strokeLineCap="round"
        android:strokeLineJoin="round" />
    <path
        android:pathData="M362,452 L604,452"
        android:strokeWidth="100"
        android:strokeColor="#FFFFFFFF"
        android:strokeLineCap="round" />
    <path
        android:pathData="M598,730 a62,62 0,1 1,124 0 a62,62 0,1 1,-124 0"
        android:fillColor="#FFFFFFFF" />
</vector>
`,
  );

  // --- app/build.gradle: androidx.webkit for MainActivity's bridge injection
  const bg = join(a, "app", "build.gradle");
  let g = read(bg);
  if (!g.includes("androidx.webkit:webkit")) {
    g = g.replace(
      /dependencies \{\n/,
      `dependencies {\n    // Feast: MainActivity exposes the native bridge to the live sites\n    implementation "androidx.webkit:webkit:$androidxWebkitVersion"\n`,
    );
    write(bg, g);
  }

  // --- app/build.gradle: release signing from android/keystore.properties (never committed)
  g = read(bg);
  if (!g.includes("feast:signing")) {
    g = g.replace(
      /apply plugin: 'com.android.application'\n/,
      `apply plugin: 'com.android.application'

// feast:signing — release builds are signed with the upload key described in
// android/keystore.properties (see mobile/README.md). Without that file,
// release builds are unsigned and debug builds use the debug key.
def keystorePropertiesFile = rootProject.file("keystore.properties")
def keystoreProperties = new Properties()
if (keystorePropertiesFile.exists()) {
    keystorePropertiesFile.withInputStream { keystoreProperties.load(it) }
}
`,
    );
    g = g.replace(
      /    buildTypes \{\n        release \{\n/,
      `    signingConfigs {
        release {
            if (keystorePropertiesFile.exists()) {
                storeFile rootProject.file(keystoreProperties['storeFile'])
                storePassword keystoreProperties['storePassword']
                keyAlias keystoreProperties['keyAlias']
                keyPassword keystoreProperties['keyPassword']
            }
        }
    }
    buildTypes {
        release {
            if (keystorePropertiesFile.exists()) {
                signingConfig signingConfigs.release
            }
`,
    );
    write(bg, g);
  }

  // --- MainActivity
  const pkg = app.appId;
  const ma = join(main, "java", ...pkg.split("."), "MainActivity.java");
  write(
    ma,
    `package ${pkg};

import android.os.Bundle;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;
import com.getcapacitor.BridgeActivity;
import com.getcapacitor.JSExport;
import com.getcapacitor.Logger;
import com.getcapacitor.PluginHandle;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * The app starts on the bundled country picker and then opens the live Feast
 * site for that country. On iOS Capacitor exposes its native bridge to every
 * page; on Android it only does so for the bundled pages, so this also adds
 * it to the Feast sites listed in server.allowNavigation (exact hosts only,
 * never the wildcard payment hosts). Generated by
 * mobile/shared/configure-native.mjs.
 */
public class MainActivity extends BridgeActivity {

    private static final String[] BRIDGE_PLUGINS = {
${BRIDGE_PLUGINS.map((p) => `        "${p}"`).join(",\n")}
    };

    @Override
    public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        exposeBridgeToFeastSites();
    }

    private void exposeBridgeToFeastSites() {
        if (bridge == null || !WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            return;
        }
        Set<String> origins = new HashSet<>();
        String[] allowed = bridge.getConfig().getAllowNavigation();
        if (allowed != null) {
            for (String host : allowed) {
                if (host == null || host.isEmpty() || host.contains("*")) continue;
                origins.add(host.startsWith("https://") ? host : "https://" + host);
            }
        }
        if (origins.isEmpty()) return;
        List<PluginHandle> plugins = new ArrayList<>();
        for (String id : BRIDGE_PLUGINS) {
            PluginHandle handle = bridge.getPlugin(id);
            if (handle != null) plugins.add(handle);
        }
        try {
            String script =
                JSExport.getGlobalJS(this, bridge.getConfig().isLoggingEnabled(), bridge.isDevMode()) +
                "\\n\\nwindow.WEBVIEW_SERVER_URL = '" +
                bridge.getLocalUrl() +
                "';\\n\\n" +
                JSExport.getBridgeJS(this) +
                "\\n\\n" +
                JSExport.getPluginJS(plugins);
            WebViewCompat.addDocumentStartJavaScript(bridge.getWebView(), script, origins);
        } catch (Exception e) {
            Logger.error("Feast", "Could not expose native features to the Feast site", e);
        }
    }
}
`,
  );

  ensureLine(join(a, ".gitignore"), "# Firebase config (download from the Firebase console; never commit)");
  ensureLine(join(a, ".gitignore"), "app/google-services.json");
  ensureLine(join(a, ".gitignore"), "# Upload keystore and its passwords");
  ensureLine(join(a, ".gitignore"), "*.jks");
  ensureLine(join(a, ".gitignore"), "*.keystore");
  ensureLine(join(a, ".gitignore"), "keystore.properties");
  console.log(`${name}: android configured (hosts: ${hosts.join(", ")})`);
}

function ios(name, app, dir) {
  const appDir = join(dir, "ios", "App", "App");

  // --- Info.plist
  const ip = join(appDir, "Info.plist");
  let p = read(ip);
  const block = `<!-- feast:begin -->
	<key>NSCameraUsageDescription</key>
	<string>Feast uses the camera to scan a restaurant's QR code.</string>
	<key>CFBundleURLTypes</key>
	<array>
		<dict>
			<key>CFBundleURLName</key>
			<string>${app.appId}</string>
			<key>CFBundleURLSchemes</key>
			<array>
				<string>${app.scheme}</string>
			</array>
		</dict>
	</array>
	<key>UIBackgroundModes</key>
	<array>
		<string>remote-notification</string>
	</array>
	<key>ITSAppUsesNonExemptEncryption</key>
	<false/>
	<!-- feast:end -->`;
  if (p.includes("<!-- feast:begin -->")) p = p.replace(/\t?<!-- feast:begin -->[\s\S]*?<!-- feast:end -->/, block.replace(/^/, "\t"));
  else p = p.replace(/<plist version="1.0">\n<dict>\n/, (s) => s + "\t" + block + "\n");
  // Phones: portrait only (the web app is designed for it); iPad keeps all orientations.
  p = p.replace(
    /(<key>UISupportedInterfaceOrientations<\/key>\s*<array>)[\s\S]*?(<\/array>)/,
    "$1\n\t\t<string>UIInterfaceOrientationPortrait</string>\n\t$2",
  );
  // arm64 only (armv7 is long gone and blocks nothing, but is misleading)
  p = p.replace(/<string>armv7<\/string>/, "<string>arm64</string>");
  write(ip, p);

  // --- Entitlements: push + associated domains (universal links)
  const ent = join(appDir, "App.entitlements");
  write(
    ent,
    `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<!-- Generated by mobile/shared/configure-native.mjs. aps-environment switches to
     production automatically when you archive for the App Store. -->
<plist version="1.0">
<dict>
	<key>aps-environment</key>
	<string>development</string>
	<key>com.apple.developer.associated-domains</key>
	<array>
${hosts.map((h) => `		<string>applinks:${h}</string>`).join("\n")}
	</array>
</dict>
</plist>
`,
  );

  // --- project.pbxproj: entitlements file + deployment target 15.5 (ML Kit)
  const pbx = join(dir, "ios", "App", "App.xcodeproj", "project.pbxproj");
  let x = read(pbx);
  x = x.replace(/IPHONEOS_DEPLOYMENT_TARGET = 15\.0;/g, "IPHONEOS_DEPLOYMENT_TARGET = 15.5;");
  // iPhone only: the web app is designed for phones, and an iPad build would
  // need its own App Store screenshots and review on iPad.
  x = x.replace(/TARGETED_DEVICE_FAMILY = "1,2";/g, 'TARGETED_DEVICE_FAMILY = 1;');
  x = x.replace(/(\n(\t+)INFOPLIST_FILE = App\/Info\.plist;)/g, (s, line, ind) =>
    s.includes("CODE_SIGN_ENTITLEMENTS") ? s : `\n${ind}CODE_SIGN_ENTITLEMENTS = App/App.entitlements;${line}`,
  );
  // de-duplicate if run twice
  x = x.replace(/(\t+CODE_SIGN_ENTITLEMENTS = App\/App\.entitlements;\n)(?:\t+CODE_SIGN_ENTITLEMENTS = App\/App\.entitlements;\n)+/g, "$1");
  write(pbx, x);

  // --- Podfile: ML Kit needs iOS 15.5
  const pod = join(dir, "ios", "App", "Podfile");
  if (existsSync(pod)) write(pod, read(pod).replace(/platform :ios, '15\.0'/, "platform :ios, '15.5'"));

  // --- AppDelegate: hand APNs registration and background pushes to the plugin
  const ad = join(appDir, "AppDelegate.swift");
  let d = read(ad);
  if (!d.includes("capacitorDidRegisterForRemoteNotifications")) {
    d = d.replace(
      /\n\}\s*$/,
      `

    // MARK: Push notifications (Firebase Cloud Messaging via @capacitor-firebase/messaging)

    func application(_ application: UIApplication, didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data) {
        NotificationCenter.default.post(name: .capacitorDidRegisterForRemoteNotifications, object: deviceToken)
    }

    func application(_ application: UIApplication, didFailToRegisterForRemoteNotificationsWithError error: Error) {
        NotificationCenter.default.post(name: .capacitorDidFailToRegisterForRemoteNotifications, object: error)
    }

    func application(_ application: UIApplication,
                     didReceiveRemoteNotification userInfo: [AnyHashable: Any],
                     fetchCompletionHandler completionHandler: @escaping (UIBackgroundFetchResult) -> Void) {
        NotificationCenter.default.post(name: Notification.Name.init("didReceiveRemoteNotification"), object: completionHandler, userInfo: userInfo)
    }
}
`,
    );
    write(ad, d);
  }

  ensureLine(join(dir, "ios", ".gitignore"), "# Firebase config (download from the Firebase console; never commit)");
  ensureLine(join(dir, "ios", ".gitignore"), "App/App/GoogleService-Info.plist");
  console.log(`${name}: ios configured (applinks: ${hosts.join(", ")})`);
}
