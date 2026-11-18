"""Adds release signing (from keystore.properties) to the Tauri-generated Android project.

Run after `tauri android init`. Idempotent; fails loudly if the generated file changed shape.
"""
import pathlib, re, sys

gradle = pathlib.Path("src-tauri/gen/android/app/build.gradle.kts")
src = gradle.read_text()
if "signingConfigs" in src:
    print("signing already configured")
    sys.exit(0)

signing = '''    signingConfigs {
        create("release") {
            val keystorePropertiesFile = rootProject.file("keystore.properties")
            val keystoreProperties = Properties()
            if (keystorePropertiesFile.exists()) {
                keystoreProperties.load(FileInputStream(keystorePropertiesFile))
            }
            keyAlias = keystoreProperties["keyAlias"] as String
            keyPassword = keystoreProperties["password"] as String
            storeFile = file(keystoreProperties["storeFile"] as String)
            storePassword = keystoreProperties["password"] as String
            storeType = "pkcs12"
        }
    }
'''

new, n1 = re.subn(r"^(\s*)buildTypes \{", lambda m: signing + m.group(0), src, count=1, flags=re.M)
new, n2 = re.subn(r'getByName\("release"\) \{', lambda m: m.group(0) + '\n            signingConfig = signingConfigs.getByName("release")', new, count=1)
if not (n1 and n2):
    sys.exit(f"could not patch {gradle}: buildTypes={n1} release={n2}")
if "import java.io.FileInputStream" not in new:
    new = "import java.io.FileInputStream\n" + new
if "import java.util.Properties" not in new:
    new = "import java.util.Properties\n" + new
gradle.write_text(new)
print("signing configured")
