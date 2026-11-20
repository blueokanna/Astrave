import 'package:tray_manager/tray_manager.dart' as tray;
import 'package:window_manager/window_manager.dart' as wm;

import 'src/rust/api.dart' as rust;

class DesktopTray {
  final window = wm.WindowManager.instance.getCurrent();
  final icon = tray.TrayIcon.create();
  final menu = tray.Menu.create();
  final showItem = tray.MenuItem.createWithLabelAndType(
    'Astrave',
    tray.MenuItemType.normal,
  );
  final quitItem = tray.MenuItem.createWithLabelAndType(
    'Quit',
    tray.MenuItemType.normal,
  );
  bool visible = false;

  void install(bool chinese) {
    if (window == null || icon == null || menu == null) return;
    icon!.icon = tray.ImageAsset.fromAsset('assets/astrave.png');
    icon!.setTooltip('Astrave');
    void show() {
      if (window!.isMinimized) window!.restore();
      window!.show();
      window!.focus();
    }

    showItem?.addListener((event) {
      if (event is tray.MenuItemClickedEvent) show();
    });
    quitItem?.addListener((event) async {
      if (event is tray.MenuItemClickedEvent) {
        await rust.stopClient();
        window!.close();
      }
    });
    menu!
      ..addItem(showItem)
      ..addSeparator()
      ..addItem(quitItem);
    localize(chinese);
    icon!
      ..setContextMenu(menu)
      ..setContextMenuTrigger(tray.ContextMenuTrigger.rightClicked)
      ..addListener((event) {
        if (event is tray.TrayIconClickedEvent) show();
      });
    visible = icon!.setVisible(true);
  }

  void localize(bool chinese) {
    showItem?.label = chinese ? '显示 Astrave' : 'Show Astrave';
    quitItem?.label = chinese ? '退出' : 'Quit';
  }

  void hide() => window?.hide();
  void dispose() {
    icon?.dispose();
    menu?.dispose();
    showItem?.dispose();
    quitItem?.dispose();
  }
}
