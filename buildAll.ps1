# Resolving paths

$metadataJson = cargo metadata --format-version 1 --no-deps
$metadata = $metadataJson | ConvertFrom-Json
$targetDir = $metadata.target_directory

New-Item -ItemType Directory "./finalOutput" -Force
$outputDir = Resolve-Path "./finalOutput"

$moonlightRoot = Resolve-Path "."
$moonlightFrontend = Join-Path -Path $moonlightRoot -ChildPath "."

if(!$moonlightRoot -or !$moonlightFrontend) {
    echo "No root directory found!"
    exit 0
}

echo "Target directory at $targetDir"
echo "Putting final output into $outputDir"
echo "Moonlight Root Directory $moonlightRoot"

$targets = @(
    "x86_64-pc-windows-gnu"
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-musl"
)

Remove-Item -Path "$outputDir/*" -Recurse -Force

echo "------------- Starting Build for Frontend -------------"
Set-Location $moonlightFrontend

New-Item -ItemType Directory "$outputDir/static" -Force | Out-Null

Remove-Item -Path "$moonlightFrontend/dist" -Recurse -Force
npm run build

Copy-Item -Path "$moonlightFrontend/dist/*" -Destination "$outputDir/static" -Recurse -Force
echo "------------- Finished Build for Frontend -------------"

Set-Location $moonlightRoot

foreach($target in $targets) {
    echo "------------- Starting Build for $target -------------"
    $messages = cross build --release --target $target --message-format=json | ForEach-Object { $_ | ConvertFrom-Json }
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
    echo "------------- Finished Build for $target -------------"

    $artifact = $messages | Where-Object { $_.reason -eq "compiler-artifact" -and $_.executable }
    $binaryPaths = $artifact | ForEach-Object { Join-Path -Path $targetDir -ChildPath ($_.executable.Substring("/target".length)) }

    $binaryPaths | ForEach-Object { Write-Host "Binary: $_" }

    echo "------------- Starting Zipping for $target -------------"
    $itemsToZip = @($binaryPaths) + "$outputDir/static"
    $archiveName = "$outputDir/moonlight-web-$target"

    if ($target -clike "*windows*") {
        # Create zip
        $zipDestination = "$archiveName.zip"
        7z a -tzip $zipDestination $itemsToZip -y
    } else {
        # Create tar.gz
        New-Item -ItemType Directory "$archiveName" -Force | Out-Null

        foreach ($item in $itemsToZip) {
            Copy-Item $item -Recurse -Destination $archiveName
        }

        $tarDestination = "$archiveName.tar"
        $gzDestination = "$archiveName.tar.gz"
        7z a -ttar $tarDestination $archiveName -y
        7z a -tgzip $gzDestination $tarDestination -y
        
        Remove-Item $tarDestination

        Remove-Item $archiveName -Recurse
    }

    echo "Created Zip file at $archiveName"
    echo "------------- Finished Zipping for $target -------------"
}

Remove-Item "$outputDir/static" -Recurse

echo "Finished!"