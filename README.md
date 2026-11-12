# Unison

Um único app leve para Windows que unifica a configuração e o monitoramento dos meus periféricos, no lugar do app da Redragon e do ATK HUB. O destaque: **mostra a porcentagem da bateria também enquanto o aparelho está carregando**, algo que os apps oficiais não fazem.

Feito com **Rust + Tauri v2** e frontend em HTML/JS puro (sem framework). O executável de release tem cerca de 3,3 MB.

## Dispositivos suportados

| Dispositivo | Conexão | Funções |
|---|---|---|
| Redragon M916 PRO 4K (sensor PAW3395) | Dongle 2.4G (`3554:F54F`) ou cabo (`3554:F54D`), via HID | Bateria, polling rate, estágios de DPI, cor de cada estágio, luz indicadora de DPI |
| ATK N9 Ultra (headset) | Dongle USB (`373B:1204`), via porta serial | Bateria (com estimativa durante a carga) |

Outros modelos das mesmas marcas podem funcionar parcialmente, mas só os acima foram testados.

## Recursos

- **Bateria em tempo real** de mouse e headset, incluindo durante a carga.
- **Ícone na bandeja** com o menor nível entre os aparelhos (cor por faixa: verde, âmbar, vermelho e azul quando carregando), tooltip e menu detalhados.
- **Configuração do mouse**: polling rate (125 Hz a 8000 Hz, conforme o hardware), quantidade de estágios, DPI (50 a 26000, passo 50), cor por estágio, estágio ativo e luz indicadora de DPI (efeito, brilho e velocidade).
- **Iniciar com o Windows** (entrada do usuário no registro, sem administrador), abrindo direto na bandeja.
- **Instância única**: abrir de novo apenas traz a janela existente para frente.
- **Baixo consumo de memória**: fechar a janela destrói o WebView2 (de ~360 MB para ~27 MB); o app continua na bandeja e recria a janela quando aberto de novo.
- Lista de dispositivos HID com um "escutador" de relatórios, útil para investigar novos aparelhos.

## Como a bateria é obtida

**Mouse.** Relatórios HID de 17 bytes (ID `0x08`, checksum `0x55 - soma(bytes 0..15)`) na interface 1, página de uso `0xFF02`. O comando `0x04` devolve porcentagem, flag de carga e tensão em mV. Com o mouse no cabo, o percentual lido tende a vir de 5 a 10 pontos acima do real.

**Headset.** Pacotes de 64 bytes (`[tamanho, cmd, sub, dados...]`) pela porta serial (9600 8N1). A requisição `02 00 03` devolve a bateria: bit 7 = carregando, bits 0-6 = porcentagem. Só que o headset reporta **100% fixo enquanto carrega** e, descarregando, em passos de 10%. Por isso o Unison **estima** o nível durante a carga: parte da mediana das últimas leituras sem carga e aplica uma taxa de carregamento (padrão 0,8%/min), recalibrada toda vez que o cabo é desconectado e salva em `headset_charge_rate.txt` na pasta de configuração do app. Valores estimados aparecem com `~`.

## Requisitos para compilar

- Windows 10/11 com WebView2 (já vem no Windows 11)
- [Rust](https://rustup.rs) (toolchain MSVC)
- Node.js (só para o CLI do Tauri)
- Visual Studio 2022 Build Tools com a carga de trabalho **Desenvolvimento para desktop com C++**

## Compilar e rodar

O script `dev.cmd` carrega o ambiente do Visual Studio 2022 antes de chamar o Tauri:

```bash
npm install

dev.cmd              # modo desenvolvimento (tauri dev)
dev.cmd build        # executável de release, sem instalador
dev.cmd installer    # instalador NSIS por usuário
```

O executável sai em `src-tauri/target/release/unison.exe`.

Para instalar manualmente, copie o `.exe` para uma pasta fixa (por exemplo `%LOCALAPPDATA%\Unison`) e marque "Iniciar com o Windows" no app. A entrada registrada é `unison.exe --minimized`.

Ao recompilar, feche o Unison antes (o `.exe` em uso fica bloqueado pelo Windows).

## Estrutura

```
src/                 frontend (index.html, style.css, main.js)
src-tauri/
  src/lib.rs         comandos Tauri, plugins e threads de monitoramento
  src/mouse.rs       bateria do mouse (HID)
  src/mouse_cfg.rs   leitura/escrita da configuração do mouse (EEPROM)
  src/headset.rs     bateria do headset (serial) e estimador de carga
  src/tray.rs        ícone, menu e tooltip da bandeja
  examples/          ferramentas de linha de comando para depuração e engenharia reversa
dev.cmd              ambiente de build do Windows
```

## Limitações conhecidas

- **Conflito com o ATK HUB:** o ATK HUB e o serviço `AtkHubAgentService` seguram a porta serial do headset. Com eles abertos, o cartão do headset mostra "porta ocupada". Feche o ATK HUB e, se quiser, desative a inicialização dele e coloque o serviço em "Manual".
- O mouse precisa estar acordado para ler ou gravar a configuração; se estiver dormindo, o app avisa.
- 4000 Hz e 8000 Hz aparecem na lista, mas o suporte do dongle nesses valores não foi verificado.
- Ainda não implementados: remapeamento de botões, macros e equalizador do headset.
- O ícone novo pode ficar escondido na área de ícones ocultos da bandeja do Windows; arraste-o para fixar.

## Segurança das gravações no mouse

Toda escrita é feita por leitura, modificação e releitura de verificação, e a configuração só é aceita quando os pares valor/CRC da EEPROM conferem. Mesmo assim, o protocolo foi obtido por engenharia reversa: use por sua conta e risco.

## Aviso

Projeto independente, sem relação com Redragon ou ATK. Nomes de marcas pertencem aos seus donos.

## Licença

Este projeto está licenciado sob a licença MIT. Consulte o arquivo [LICENSE](LICENSE).
