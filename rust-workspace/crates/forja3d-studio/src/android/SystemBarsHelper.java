package dev.aethravox.forja3d;

import android.app.Activity;
import android.graphics.Insets;
import android.os.Build;
import android.view.DisplayCutout;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;

public final class SystemBarsHelper {
    public static void enableImmersive(final Activity activity) {
        if (activity == null) {
            return;
        }
        activity.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                Window window = activity.getWindow();
                if (window == null) {
                    return;
                }
                if (Build.VERSION.SDK_INT >= 28) {
                    WindowManager.LayoutParams lp = window.getAttributes();
                    lp.layoutInDisplayCutoutMode =
                        WindowManager.LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                    window.setAttributes(lp);
                }
                window.addFlags(WindowManager.LayoutParams.FLAG_FULLSCREEN);
                View decor = window.getDecorView();
                if (decor == null) {
                    return;
                }
                if (Build.VERSION.SDK_INT >= 30) {
                    window.setDecorFitsSystemWindows(false);
                    WindowInsetsController ctrl = window.getInsetsController();
                    if (ctrl != null) {
                        ctrl.hide(WindowInsets.Type.systemBars());
                        ctrl.setSystemBarsBehavior(
                            WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
                        );
                    }
                } else {
                    decor.setSystemUiVisibility(
                        View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                            | View.SYSTEM_UI_FLAG_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                            | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                            | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                            | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION
                    );
                }
            }
        });
    }

    public static int[] getSafeInsetsPx(final Activity activity) {
        int[] out = new int[] { 0, 0, 0, 0 };
        if (activity == null) {
            return out;
        }
        Window window = activity.getWindow();
        if (window == null) {
            return out;
        }
        View decor = window.getDecorView();
        if (decor == null) {
            return out;
        }
        WindowInsets wi = decor.getRootWindowInsets();
        if (wi == null) {
            return out;
        }

        if (Build.VERSION.SDK_INT >= 30) {
            Insets ins = wi.getInsets(
                WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout()
            );
            out[0] = ins.left;
            out[1] = ins.top;
            out[2] = ins.right;
            out[3] = ins.bottom;
        } else {
            out[0] = wi.getSystemWindowInsetLeft();
            out[1] = wi.getSystemWindowInsetTop();
            out[2] = wi.getSystemWindowInsetRight();
            out[3] = wi.getSystemWindowInsetBottom();
            if (Build.VERSION.SDK_INT >= 28) {
                DisplayCutout cutout = wi.getDisplayCutout();
                if (cutout != null) {
                    out[0] = Math.max(out[0], cutout.getSafeInsetLeft());
                    out[1] = Math.max(out[1], cutout.getSafeInsetTop());
                    out[2] = Math.max(out[2], cutout.getSafeInsetRight());
                    out[3] = Math.max(out[3], cutout.getSafeInsetBottom());
                }
            }
        }
        return out;
    }
}
