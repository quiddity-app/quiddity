import org.gradle.testing.jacoco.plugins.JacocoTaskExtension
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "uk.co.aylett.quiddity.sync"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    buildTypes {
        getByName("debug") {
            // `createDebugUnitTestCoverageReport` writes the JaCoCo report
            // that CI hands to SonarCloud.
            enableUnitTestCoverage = true
        }
    }

    testCoverage {
        // The default is older than the JDK the tests run on.
        jacocoVersion = "0.8.12"
    }

    testOptions {
        unitTests {
            // Robolectric reads the manifest and resources from the build.
            isIncludeAndroidResources = true
            all { test ->
                // JaCoCo and Robolectric each rewrite classes: this is what
                // lets them share a JVM.
                (test.extensions.findByName("jacoco") as? JacocoTaskExtension)?.apply {
                    isIncludeNoLocationClasses = true
                    excludes = listOf("jdk.internal.*")
                }
            }
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_1_8
    }
}

// The versions this resolves must not move under a build: `gradle.lockfile`
// holds the ones that were, and a difference fails it. After changing a
// dependency, or when the Tauri CLI moves the generated project's plugins on,
// refresh it from `src-tauri/gen/android` with
//
//     ./gradlew :tauri-plugin-background-sync:dependencies --write-locks
//
// CI checks that it is current. The generated project is built by the Tauri
// CLI and isn't ours, so only this module's configurations are locked.
dependencyLocking {
    lockAllConfigurations()
}

configurations.configureEach {
    resolutionStrategy {
        if (isCanBeConsumed && !isCanBeResolved && !isCanBeDeclared) {
            failOnVersionConflict()
        }
        if (!isCanBeConsumed && !isCanBeResolved && isCanBeDeclared) {
            failOnVersionConflict()
        }
        // Not `failOnNonReproducibleResolution()`, which Gradle refuses to
        // combine with locking ("you must choose between the two modes"): the
        // lock is what keeps resolution reproducible here.
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.9.0")
    implementation("androidx.work:work-runtime:2.9.1")
    implementation(project(":tauri-android"))

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.14.1")
    testImplementation("androidx.test:core:1.6.1")
    testImplementation("androidx.work:work-testing:2.9.1")
}
