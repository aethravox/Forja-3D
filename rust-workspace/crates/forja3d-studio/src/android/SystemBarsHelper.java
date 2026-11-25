package dev.aethravox.forja3d;

import android.app.Activity;
import android.app.Fragment;
import android.app.FragmentManager;
import android.content.ActivityNotFoundException;
import android.content.ContentResolver;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Insets;
import android.net.Uri;
import android.os.Build;
import android.provider.OpenableColumns;
import android.view.DisplayCutout;
import android.view.View;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import android.view.WindowManager;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;

public final class SystemBarsHelper {
    // 0 = idle, 1 = pending, 2 = picked, 3 = cancelled, 4 = error
    private static volatile int sPickState = 0;
    private static volatile String sPickedName = null;
    private static volatile byte[] sPickedBytes = null;
    private static volatile String sPickedError = null;

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

    public static String openFilePicker(final Activity activity) {
        if (activity == null) {
            return "ERR:Actividad de Android nula";
        }
        sPickedName = null;
        sPickedBytes = null;
        sPickedError = null;
        sPickState = 1;
        try {
            activity.runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    try {
                        FragmentManager fm = activity.getFragmentManager();
                        if (fm == null) {
                            sPickedError = "Sin FragmentManager en Android";
                            sPickState = 4;
                            return;
                        }
                        Fragment prev = fm.findFragmentByTag("forja3d_picker");
                        if (prev != null) {
                            fm.beginTransaction().remove(prev).commitAllowingStateLoss();
                        }
                        PickerFragment frag = new PickerFragment();
                        fm.beginTransaction().add(frag, "forja3d_picker").commitAllowingStateLoss();
                        fm.executePendingTransactions();
                    } catch (Throwable t) {
                        sPickedError = "Error abriendo selector: " + t.getClass().getSimpleName() + ": " + t.getMessage();
                        sPickState = 4;
                    }
                }
            });
            return "OK";
        } catch (Throwable t) {
            sPickState = 0;
            return "ERR:" + t.getClass().getSimpleName() + ": " + t.getMessage();
        }
    }

    public static int pollPickState() {
        return sPickState;
    }

    public static String takePickedName() {
        String n = sPickedName;
        sPickedName = null;
        return n;
    }

    public static byte[] takePickedBytes() {
        byte[] b = sPickedBytes;
        sPickedBytes = null;
        sPickState = 0;
        return b;
    }

    public static String takePickedError() {
        String e = sPickedError;
        sPickedError = null;
        sPickState = 0;
        return e;
    }

    public static void clearPickState() {
        sPickedName = null;
        sPickedBytes = null;
        sPickedError = null;
        sPickState = 0;
    }

    static void deliverResult(final String name, final byte[] data, final String error) {
        if (error != null) {
            sPickedError = error;
            sPickState = 4;
        } else if (data != null) {
            sPickedName = name;
            sPickedBytes = data;
            sPickState = 2;
        } else {
            sPickState = 3;
        }
    }

    @SuppressWarnings("deprecation")
    public static class PickerFragment extends Fragment {
        private static final int REQ_CODE = 40921;
        private boolean launched = false;
        private boolean delivered = false;

        public PickerFragment() {}

        private void finishOnce(final String name, final byte[] data, final String error) {
            if (delivered) {
                return;
            }
            delivered = true;
            deliverResult(name, data, error);
            try {
                FragmentManager fm = getFragmentManager();
                if (fm != null) {
                    fm.beginTransaction().remove(this).commitAllowingStateLoss();
                }
            } catch (Throwable ignored) {}
        }

        @Override
        public void onResume() {
            super.onResume();
            if (launched) {
                return;
            }
            launched = true;
            try {
                Intent openDoc = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                openDoc.addCategory(Intent.CATEGORY_OPENABLE);
                openDoc.setType("*/*");
                openDoc.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                try {
                    startActivityForResult(openDoc, REQ_CODE);
                } catch (ActivityNotFoundException e) {
                    Intent getContent = new Intent(Intent.ACTION_GET_CONTENT);
                    getContent.addCategory(Intent.CATEGORY_OPENABLE);
                    getContent.setType("*/*");
                    startActivityForResult(getContent, REQ_CODE);
                }
            } catch (Throwable t) {
                finishOnce(null, null, "No se encontró gestor de archivos: " + t.getMessage());
            }
        }

        @Override
        public void onActivityResult(final int requestCode, final int resultCode, final Intent data) {
            super.onActivityResult(requestCode, resultCode, data);
            if (requestCode != REQ_CODE) {
                return;
            }
            if (resultCode != Activity.RESULT_OK || data == null || data.getData() == null) {
                finishOnce(null, null, null);
                return;
            }
            final Activity act = getActivity();
            final Uri uri = data.getData();
            if (act == null || uri == null) {
                finishOnce(null, null, "Actividad no disponible tras seleccionar archivo");
                return;
            }
            final ContentResolver resolver = act.getContentResolver();
            new Thread(new Runnable() {
                @Override
                public void run() {
                    try {
                        String fileName = queryDisplayName(resolver, uri);
                        if (fileName == null || fileName.isEmpty()) {
                            String seg = uri.getLastPathSegment();
                            fileName = (seg != null && !seg.isEmpty()) ? seg : "modelo.glb";
                        }
                        InputStream is = resolver.openInputStream(uri);
                        if (is == null) {
                            finishOnce(null, null, "No se pudo abrir el stream del archivo");
                            return;
                        }
                        ByteArrayOutputStream bos = new ByteArrayOutputStream();
                        byte[] buf = new byte[16384];
                        int n;
                        while ((n = is.read(buf)) != -1) {
                            bos.write(buf, 0, n);
                        }
                        is.close();
                        finishOnce(fileName, bos.toByteArray(), null);
                    } catch (Throwable t) {
                        finishOnce(null, null, "Error leyendo archivo: " + t.getMessage());
                    }
                }
            }, "Forja3DFileReader").start();
        }

        private static String queryDisplayName(final ContentResolver resolver, final Uri uri) {
            Cursor cursor = null;
            try {
                cursor = resolver.query(
                    uri,
                    new String[] { OpenableColumns.DISPLAY_NAME },
                    null,
                    null,
                    null
                );
                if (cursor != null && cursor.moveToFirst()) {
                    int idx = cursor.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                    if (idx >= 0) {
                        return cursor.getString(idx);
                    }
                }
            } catch (Throwable ignored) {
            } finally {
                if (cursor != null) {
                    cursor.close();
                }
            }
            return null;
        }
    }
}
