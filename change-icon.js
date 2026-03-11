import path from 'path';
import { fileURLToPath } from 'url';
import fs from 'fs';
import pngToIco from 'png-to-ico';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

async function main() {
    const { rcedit } = await import('rcedit');
    const exePath = path.join(__dirname, 'src-tauri', 'binaries', 'bypax-proxy-x86_64-pc-windows-msvc.exe');
    const pngPath = path.join(__dirname, 'public', 'bypax-engine.png');
    const iconPath = path.join(__dirname, 'src-tauri', 'icons', 'bypax-engine.ico');
    
    // Also convert uninstall.png to uninstall.ico
    const uninstallPngPath = path.join(__dirname, 'public', 'uninstall.png');
    const uninstallIconPath = path.join(__dirname, 'src-tauri', 'icons', 'uninstall.ico');

    console.log(`Converting ${pngPath} to ICO...`);
    try {
        const buf = await pngToIco(pngPath);
        fs.writeFileSync(iconPath, buf);
        console.log(`Created temporary ICO at ${iconPath}`);
        
        console.log(`Converting ${uninstallPngPath} to ICO...`);
        const uninstallBuf = await pngToIco(uninstallPngPath);
        fs.writeFileSync(uninstallIconPath, uninstallBuf);
        console.log(`Created temporary ICO at ${uninstallIconPath}`);
    } catch (err) {
        console.error('Failed to convert PNG to ICO:', err);
        return;
    }

    console.log(`Updating icon for ${exePath} using ${iconPath}`);

    try {
        await rcedit(exePath, {
            icon: iconPath,
            'version-string': {
                ProductName: 'BypaxDPI',
                FileDescription: 'BypaxDPI Service',
                CompanyName: 'ConsolAktif',
                LegalCopyright: 'Copyright © 2026 ConsolAktif'
            }
        });
        console.log('Icon updated successfully!');
    } catch (e) {
        console.error('Failed to update icon:', e);
    }
}

main();
