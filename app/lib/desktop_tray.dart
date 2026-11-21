import 'package:tray_manager/tray_manager.dart' as tray;
import 'package:window_manager/window_manager.dart' as wm;

import 'src/rust/api.dart' as rust;
import 'design.dart';

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

  void install(LocalText text) {
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
    localize(text);
    icon!
      ..setContextMenu(menu)
      ..setContextMenuTrigger(tray.ContextMenuTrigger.rightClicked)
      ..addListener((event) {
        if (event is tray.TrayIconClickedEvent) show();
      });
    visible = icon!.setVisible(true);
  }

  void localize(LocalText text) {
    showItem?.label = text('Show Astrave', '显示 Astrave');
    quitItem?.label = text('Quit', '退出');
  }

  void hide() => window?.hide();
  void dispose() {
    icon?.dispose();
    menu?.dispose();
    showItem?.dispose();
    quitItem?.dispose();
  }
}
