# AlterDNS — Copyright (C) 2026 Furkan Okan
# SPDX-License-Identifier: GPL-3.0-or-later
#
# S0 envanteri (CONNECTIVITY_RESEARCH §5, H6): yalnızca okur, sistemde hiçbir şeyi değiştirmez.
# Çıktı JSON olarak standart çıkışa yazılır; `cargo xtask lab inventory` bunu lab/results/ altına
# kaydeder (git dışı). Kişisel ağ verisi içerir; public depoya girmez.
# Bazı bölümler (Defender geçmişi) yönetici terminali ister; erişilemezse bölüme hata yazılır.
# -HistoryDirs: '|' ile ayrılmış klasör listesi (lab/local/lab.local.toml, git dışı).
param(
    [string]$HistoryDirs = ''
)

$ErrorActionPreference = 'Stop'

function Get-Section([scriptblock]$Block) {
    try { @(& $Block) } catch { @{ hata = $_.Exception.Message } }
}

$inventory = [ordered]@{
    zaman = (Get-Date).ToString('o')
    hop_limit = Get-Section { [ordered]@{ v4 = (Get-NetIPv4Protocol).DefaultHopLimit; v6 = (Get-NetIPv6Protocol).DefaultHopLimit } }
    arayuz_hop_limit = Get-Section { Get-NetIPInterface | Select-Object InterfaceAlias, AddressFamily, CurrentHopLimit, NlMtu, ConnectionState }
    adaptorler = Get-Section { Get-NetAdapter | Select-Object Name, InterfaceDescription, Status, MtuSize }
    ag_profili = Get-Section { Get-NetConnectionProfile | Select-Object InterfaceAlias, NetworkCategory, IPv4Connectivity, IPv6Connectivity }
    ip_yapilandirmasi = Get-Section { Get-NetIPConfiguration | Select-Object InterfaceAlias, IPv4Address, IPv4DefaultGateway, IPv6Address, DNSServer }
    dns_sunuculari = Get-Section { Get-DnsClientServerAddress | Where-Object { $_.ServerAddresses } | Select-Object InterfaceAlias, AddressFamily, ServerAddresses }
    doh_tablosu = Get-Section { Get-DnsClientDohServerAddress | Select-Object ServerAddress, DohTemplate, AutoUpgrade, AllowFallbackToUdp }
    servisler = Get-Section { Get-Service -Name 'GoodbyeDPI', 'WinDivert', 'WinDivert14' -ErrorAction SilentlyContinue | Select-Object Name, Status, StartType }
    suruculer = Get-Section { Get-CimInstance -ClassName 'Win32_SystemDriver' -Filter "Name LIKE 'WinDivert%'" | Select-Object Name, State, StartMode, PathName }
    gorevler = Get-Section { Get-ScheduledTask | Where-Object { $_.TaskName -like '*GoodbyeDPI*' } | Select-Object TaskName, State }
    islemler = Get-Section { Get-Process -Name 'goodbyedpi', 'winws' -ErrorAction SilentlyContinue | Select-Object Name, Id }
    olay_gunlugu = Get-Section { Get-WinEvent -FilterHashtable @{ LogName = 'System'; Id = 7000, 7023, 7031, 7034 } -MaxEvents 1000 -ErrorAction SilentlyContinue | Where-Object { $_.Message -match 'GoodbyeDPI|WinDivert' } | Select-Object TimeCreated, Id, ProviderName, Message }
    defender_tespitleri = Get-Section { Get-MpThreatDetection | Select-Object ThreatID, InitialDetectionTime, ActionSuccess, Resources }
    defender_tehditleri = Get-Section { Get-MpThreat | Select-Object ThreatID, ThreatName }
    vbs_hvci = Get-Section { Get-CimInstance -ClassName 'Win32_DeviceGuard' -Namespace 'root/Microsoft/Windows/DeviceGuard' | Select-Object VirtualizationBasedSecurityStatus, SecurityServicesRunning }
    tarihsel_loglar = Get-Section { foreach ($d in ($HistoryDirs -split '\|' | Where-Object { $_ })) { Get-ChildItem -LiteralPath $d -Recurse -File -ErrorAction SilentlyContinue | Select-Object FullName, Length, LastWriteTime } }
}

$json = $inventory | ConvertTo-Json -Depth 5
# Konsol kod sayfasından bağımsız olsun diye ASCII dışı karakterler \uXXXX olarak kaçışlanır
# (konsolun kod sayfası değiştirilmez).
-join ($json.ToCharArray() | ForEach-Object { if ([int]$_ -lt 128) { [string]$_ } else { '\u{0:x4}' -f [int]$_ } })
