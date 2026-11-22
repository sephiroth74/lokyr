import com.pavelfatin.typometer.benchmark.*;
import com.pavelfatin.typometer.screen.ScreenAccessor;
import java.awt.Robot;
import java.awt.event.KeyEvent;
import java.awt.event.InputEvent;
import java.io.*;
import java.util.*;
import java.util.concurrent.*;

/**
 * Typometer 1.1.0's own benchmark, of its jar, driven without its window,
 * so that a script runs it in each terminal in turn, of Typometer's
 * settings: 200 keys 150 ms apart, no pauses, the native API off and the
 * synchronous mode. Its arguments: `check`, the permissions to read the
 * screen and post keys; `type <text>`, of letters, spaces and newlines;
 * `key <enter|cmd+n|ctrl+c|ctrl+d>`; `click <x> <y>`, in points; and
 * `bench <title> <out.csv> [count] [delay]`, a column of the delays, in
 * milliseconds, under its title, as Typometer's CSV holds it.
 *
 *     javac -cp "$JAR" -d /tmp/lokyr/typo tools/bench/typometer/Harness.java
 *     java -Dapple.awt.UIElement=true -cp "$JAR:/tmp/lokyr/typo" Harness check
 */
public class Harness {
    public static void main(String[] args) throws Exception {
        switch (args[0]) {
            case "check" -> {
                System.out.println("screen capture: " + ScreenAccessor.isScreenCapturePermissionGranted());
                System.out.println("input: " + ScreenAccessor.isInputPermissionGranted());
                System.out.println("native api: " + ScreenAccessor.isNativeApiSupported());
            }
            case "type" -> { Robot r = new Robot(); r.setAutoDelay(15); for (char c : args[1].toCharArray()) typeChar(r, c); }
            case "key" -> { Robot r = new Robot(); r.setAutoDelay(30); key(r, args[1]); }
            case "click" -> { Robot r = new Robot(); r.setAutoDelay(50); r.mouseMove(Integer.parseInt(args[1]), Integer.parseInt(args[2])); r.mousePress(InputEvent.BUTTON1_DOWN_MASK); r.mouseRelease(InputEvent.BUTTON1_DOWN_MASK); }
            case "bench" -> bench(args);
            default -> throw new IllegalArgumentException(args[0]);
        }
        System.exit(0);
    }

    static void key(Robot r, String name) {
        switch (name) {
            case "enter" -> { r.keyPress(KeyEvent.VK_ENTER); r.keyRelease(KeyEvent.VK_ENTER); }
            case "cmd+n" -> { r.keyPress(KeyEvent.VK_META); r.keyPress(KeyEvent.VK_N); r.keyRelease(KeyEvent.VK_N); r.keyRelease(KeyEvent.VK_META); }
            case "ctrl+c" -> { r.keyPress(KeyEvent.VK_CONTROL); r.keyPress(KeyEvent.VK_C); r.keyRelease(KeyEvent.VK_C); r.keyRelease(KeyEvent.VK_CONTROL); }
            case "ctrl+d" -> { r.keyPress(KeyEvent.VK_CONTROL); r.keyPress(KeyEvent.VK_D); r.keyRelease(KeyEvent.VK_D); r.keyRelease(KeyEvent.VK_CONTROL); }
            default -> throw new IllegalArgumentException(name);
        }
    }

    static void typeChar(Robot r, char c) {
        boolean shift = false; int code;
        switch (c) {
            case ' ' -> code = KeyEvent.VK_SPACE;
            case '>' -> { code = KeyEvent.VK_PERIOD; shift = true; }
            case '/' -> code = KeyEvent.VK_SLASH;
            case '\n' -> code = KeyEvent.VK_ENTER;
            default -> code = KeyEvent.getExtendedKeyCodeForChar(c);
        }
        if (shift) r.keyPress(KeyEvent.VK_SHIFT);
        r.keyPress(code); r.keyRelease(code);
        if (shift) r.keyRelease(KeyEvent.VK_SHIFT);
    }

    static void bench(String[] args) throws Exception {
        String title = args[1], out = args[2];
        int count = args.length > 3 ? Integer.parseInt(args[3]) : 200;
        int delay = args.length > 4 ? Integer.parseInt(args[4]) : 150;
        // Typometer's settings: 200 characters, 150 ms apart, no pauses,
        // the native API off, the synchronous mode.
        Parameters parameters = new Parameters(count, delay, false, 50, 1000);
        ScreenAccessor accessor = ScreenAccessor.create(false);
        Benchmark benchmark = Benchmark.create();
        List<Double> results = Collections.synchronizedList(new ArrayList<>());
        CountDownLatch done = new CountDownLatch(1);
        String[] error = {null};
        benchmark.run(parameters, false, accessor, new BenchmarkListener() {
            public void onStart(Parameters p) { System.err.println("start"); }
            public void onPhase(String s) { System.err.println(s); }
            public void onResult(double d) { results.add(d); }
            public void onError(String s) { error[0] = s; System.err.println("error: " + s); done.countDown(); }
            public void onFinish() { done.countDown(); }
        });
        done.await(10, TimeUnit.MINUTES);
        benchmark.dispose();
        accessor.dispose();
        try (PrintWriter w = new PrintWriter(new FileWriter(out))) {
            w.println("\"" + title + "\"");
            for (double d : results) w.println(String.format(Locale.ROOT, "%.2f", d));
        }
        System.err.println(title + ": " + results.size() + " keys" + (error[0] != null ? ", " + error[0] : ""));
        if (error[0] != null) System.exit(2);
    }
}
