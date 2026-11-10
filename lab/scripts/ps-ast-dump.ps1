# AlterDNS — Copyright (C) 2026 Furkan Okan
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Bir PowerShell betiğinin sözdizimi ağacını (AST) çalıştırmadan döker. `xtask` salt okunur
# denetimi (scenario::ps_script_violations) bu çıktıyı izin listesiyle karşılaştırır.
# Çıktı: satır başına `tür|değer`.
param(
    [Parameter(Mandatory = $true)][string]$Path
)

$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($Path, [ref]$tokens, [ref]$parseErrors)
foreach ($e in $parseErrors) { 'parse|' + $e.Message }
foreach ($n in $ast.FindAll({ $true }, $true)) {
    switch ($n.GetType().Name) {
        'CommandAst' {
            $name = $n.GetCommandName()
            if ($name) { 'command|' + $name } else { 'dynamic|' + $n.Extent.Text }
        }
        'InvokeMemberExpressionAst' { 'member|' + $n.Member.Extent.Text }
        'AssignmentStatementAst' {
            $left = $n.Left
            if ($left.GetType().Name -eq 'VariableExpressionAst' -and $left.VariablePath.IsUnqualified) {
                'assign-var|' + $left.VariablePath.UserPath
            } else {
                'assign|' + $left.Extent.Text
            }
        }
        'FileRedirectionAst' { 'redirect|' + $n.Extent.Text }
    }
}
