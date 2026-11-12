# AlterDNS lab - korunan degerlerin anlik goruntusu (ADR-014). Salt okunur.
# Her lab senaryosu basinda ve sonunda calisir; xtask iki ciktiyi karsilastirir.
# Yalnizca kararli alanlar alinir (ornegin Wi-Fi hizina gore degisen otomatik metrik yok).
$ErrorActionPreference = 'SilentlyContinue'
$snapshot = [ordered]@{
    hop_limit = [ordered]@{ v4 = (Get-NetIPv4Protocol).DefaultHopLimit; v6 = (Get-NetIPv6Protocol).DefaultHopLimit }
    arayuzler = @(Get-NetIPInterface | Where-Object { $_.ConnectionState -eq 'Connected' -and $_.InterfaceAlias -ne 'AlterDNS-Spike' } | Sort-Object InterfaceAlias, AddressFamily | Select-Object InterfaceAlias, AddressFamily, AutomaticMetric, NlMtu, CurrentHopLimit)
    varsayilan_rotalar = @(Get-NetRoute -DestinationPrefix '0.0.0.0/0', '::/0' | Sort-Object DestinationPrefix, InterfaceAlias, NextHop | Select-Object DestinationPrefix, InterfaceAlias, NextHop, RouteMetric)
    dns = @(Get-DnsClientServerAddress | Where-Object { $_.ServerAddresses -and $_.InterfaceAlias -ne 'AlterDNS-Spike' } | Sort-Object InterfaceAlias, AddressFamily | Select-Object InterfaceAlias, AddressFamily, ServerAddresses)
    doh = @(Get-DnsClientDohServerAddress | Sort-Object ServerAddress | Select-Object ServerAddress, DohTemplate, AutoUpgrade, AllowFallbackToUdp)
    nrpt = @(Get-DnsClientNrptRule | Sort-Object Name | Select-Object Name, Namespace, NameServers)
    proxy_wininet = Get-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' | Select-Object ProxyEnable, ProxyServer, ProxyOverride, AutoConfigURL, AutoDetect
    proxy_winhttp = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Internet Settings\Connections' | Select-Object WinHttpSettings
    guvenlik_duvari = @(Get-NetFirewallProfile | Sort-Object Name | Select-Object Name, Enabled, DefaultInboundAction, DefaultOutboundAction)
}
$snapshot | ConvertTo-Json -Compress -Depth 5
