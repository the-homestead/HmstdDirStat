//! The app's asset source: gpui-kit's default bundle plus the extra Lucide icons the app uses.
//! gpui-kit embeds only its default icons; any other icon renders blank unless it is listed here.

use gpui_kit::*;

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        ChartPie,
        Disc,
        FolderTree,
        HardDrive,
        Layers,
        LayoutDashboard,
        LayoutGrid,
        FileArchive,
        FileCode,
        FileImage,
        FileMusic,
        FilePlay,
        FileSpreadsheet
    ]
);

/// Default bundle first, then the extra icons.
pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
