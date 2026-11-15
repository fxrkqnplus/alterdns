# AlterDNS lab - korunan degerlerin anlik goruntusu (ADR-014). Salt okunur.
# Her lab senaryosu basinda ve sonunda calisir; xtask iki ciktiyi karsilastirir.
# Yalnizca kararli alanlar alinir (ornegin Wi-Fi hizina gore degisen otomatik metrik yok).
# Okunamayan bolum {"hata": ...} olarak yazilir; xtask bu durumda senaryoyu baslatmaz ya da
# basarisiz sayar (2026-11-15 guvenlik incelemesi). Kayit defteri anahtari yoksa deger bos kalir.
$ErrorActionPreference = 'Stop'
function Get-Section([scriptblock]$Block) {
    try { & $Block } catch { [ordered]@{ hata = $_.Exception.Message } }
}
$snapshot = [ordered]@{
    hop_limit = Get-Section { [ordered]@{ v4 = (Get-NetIPv4Protocol).DefaultHopLimit; v6 = (Get-NetIPv6Protocol).DefaultHopLimit } }
    arayuzler = @(Get-Section { Get-NetIPInterface | Where-Object { $_.ConnectionState -eq 'Connected' -and $_.InterfaceAlias -ne 'AlterDNS-Spike' } | Sort-Object InterfaceAlias, AddressFamily | Select-Object InterfaceAlias, AddressFamily, AutomaticMetric, NlMtu, CurrentHopLimit })
    varsayilan_rotalar = @(Get-Section { Get-NetRoute | Where-Object { $_.DestinationPrefix -eq '0.0.0.0/0' -or $_.DestinationPrefix -eq '::/0' } | Sort-Object DestinationPrefix, InterfaceAlias, NextHop | Select-Object DestinationPrefix, InterfaceAlias, NextHop, RouteMetric })
    dns = @(Get-Section { Get-DnsClientServerAddress | Where-Object { $_.ServerAddresses -and $_.InterfaceAlias -ne 'AlterDNS-Spike' } | Sort-Object InterfaceAlias, AddressFamily | Select-Object InterfaceAlias, AddressFamily, ServerAddresses })
    doh = @(Get-Section { if (Get-Command -Name Get-DnsClientDohServerAddress -ErrorAction SilentlyContinue) { Get-DnsClientDohServerAddress | Sort-Object ServerAddress | Select-Object ServerAddress, DohTemplate, AutoUpgrade, AllowFallbackToUdp } else { 'desteklenmiyor' } })
    nrpt = @(Get-Section { Get-DnsClientNrptRule | Sort-Object Name | Select-Object Name, Namespace, NameServers })
    proxy_wininet = Get-Section { Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue | Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL, AutoDetect }
    proxy_winhttp = Get-Section { Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Internet Settings\Connections' -ErrorAction SilentlyContinue | Select-Object WinHttpSettings }
    guvenlik_duvari = @(Get-Section { Get-NetFirewallProfile | Sort-Object Name | Select-Object Name, Enabled, DefaultInboundAction, DefaultOutboundAction })
}
$snapshot | ConvertTo-Json -Compress -Depth 5
