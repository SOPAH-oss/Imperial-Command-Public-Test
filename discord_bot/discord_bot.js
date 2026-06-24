const fs = require('fs');
const path = require('path');
const {
  Client,
  GatewayIntentBits,
  REST,
  Routes,
  SlashCommandBuilder,
  PermissionFlagsBits,
} = require('discord.js');

const root = path.resolve(__dirname, '..');
const configPath = path.join(__dirname, 'discord_config.json');
const exampleConfigPath = path.join(__dirname, 'discord_config.example.json');
const bankPath = path.join(root, 'bank.json');
const legacyCasinoPath = path.join(root, 'casino.json');
const shopPath = path.join(root, 'shop_items.json');
const shopAuditPath = path.join(root, 'shop_role_audit.json');
const usersPath = path.join(root, 'users.json');
const blackjackSessions = new Map();

function readJson(file, fallback) {
  try {
    return JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (_) {
    return fallback;
  }
}

function writeJson(file, value) {
  fs.writeFileSync(file, JSON.stringify(value, null, 2));
}

function appendJsonArray(file, entry) {
  const rows = readJson(file, []);
  rows.push(entry);
  writeJson(file, rows);
}

function config() {
  if (!fs.existsSync(configPath) && fs.existsSync(exampleConfigPath)) {
    fs.copyFileSync(exampleConfigPath, configPath);
  }
  const cfg = readJson(configPath, {});
  if (!cfg.token || cfg.token.includes('PUT_')) {
    throw new Error('Set discord_bot/discord_config.json token, client_id, and guild_id first.');
  }
  return cfg;
}

function cleanUser(user) {
  return String(user || '').trim().toLowerCase();
}

function loadUsers() {
  return readJson(usersPath, []);
}

function sameName(a, b) {
  return cleanUser(a) === cleanUser(b);
}

function discordNamesFor(interaction) {
  const names = [
    interaction.user?.id,
    interaction.user?.username,
    interaction.user?.globalName,
    interaction.member?.displayName,
    interaction.user?.tag,
  ];
  return names.filter(Boolean).map(String);
}

function bankUserForInteraction(interaction) {
  const names = discordNamesFor(interaction);
  const users = loadUsers();
  const linked = users.find(user => {
    const discord = String(user.discord_name || '').trim();
    return discord && names.some(name => sameName(discord, name));
  });
  if (linked) {
    return linked.minecraft_name || linked.username;
  }
  return interaction.member?.displayName || interaction.user.username;
}

function loadBank() {
  return readJson(bankPath, readJson(legacyCasinoPath, {}));
}

function saveBank(bank) {
  writeJson(bankPath, bank);
}

function balance(user) {
  const key = cleanUser(user);
  const bank = loadBank();
  if (bank[key] == null) {
    bank[key] = 1000;
    saveBank(bank);
  }
  return { username: key, credits: bank[key] };
}

function addCredits(user, amount) {
  const key = cleanUser(user);
  const bank = loadBank();
  if (bank[key] == null) bank[key] = 1000;
  bank[key] += amount;
  saveBank(bank);
  return bank[key];
}

function charge(user, amount) {
  if (!Number.isInteger(amount) || amount < 1) throw new Error('Bet must be at least 1.');
  if (amount > 100000) throw new Error('Bet is too large.');
  const key = cleanUser(user);
  const bank = loadBank();
  if (bank[key] == null) bank[key] = 1000;
  if (bank[key] < amount) throw new Error('Not enough Credits.');
  bank[key] -= amount;
  saveBank(bank);
  return bank[key];
}

function rand(maxExclusive) {
  return Math.floor(Math.random() * maxExclusive);
}

function playSlots(user, bet) {
  charge(user, bet);
  const symbols = ['⬛', '⬜', '⚫', '⚪', '♠️', '♣️'];
  const reels = [symbols[rand(symbols.length)], symbols[rand(symbols.length)], symbols[rand(symbols.length)]];
  let payout = 0;
  if (reels[0] === reels[1] && reels[1] === reels[2]) payout = reels[0] === '⬛' ? bet * 25 : bet * 10;
  else if (reels[0] === reels[1] || reels[0] === reels[2] || reels[1] === reels[2]) payout = bet * 2;
  if (payout > 0) addCredits(user, payout);
  const b = balance(user);
  return `${reels.join('  ')}\n${payout > 0 ? `You won ${payout}` : `You lost ${bet}`} Credits. Balance: ${b.credits}`;
}

function rouletteColor(n) {
  if (n === 0) return 'green';
  return [1,3,5,7,9,12,14,16,18,19,21,23,25,27,30,32,34,36].includes(n) ? 'red' : 'black';
}

function playRoulette(user, bet, choice) {
  charge(user, bet);
  const number = rand(37);
  const color = rouletteColor(number);
  const c = String(choice || '').toLowerCase();
  let won = false;
  if (['red', 'black', 'green'].includes(c)) won = color === c;
  else if (c === 'odd') won = number !== 0 && number % 2 === 1;
  else if (c === 'even') won = number !== 0 && number % 2 === 0;
  else {
    const picked = Number(c);
    if (!Number.isInteger(picked) || picked < 0 || picked > 36) throw new Error('Choice must be red, black, green, odd, even, or 0-36.');
    won = picked === number;
  }
  const payout = won ? ((c === 'green' || /^\d+$/.test(c)) ? bet * 36 : bet * 2) : 0;
  if (payout > 0) addCredits(user, payout);
  const b = balance(user);
  const dot = color === 'red' ? '🔴' : color === 'black' ? '⚫' : '🟢';
  return `🎡 ${number} ${dot} ${color}\n${won ? `You won ${payout}` : `You lost ${bet}`} Credits. Balance: ${b.credits}`;
}

function deck() {
  const cards = [];
  for (let s = 0; s < 4; s++) for (let c = 1; c <= 13; c++) cards.push(c);
  for (let i = cards.length - 1; i > 0; i--) {
    const j = rand(i + 1);
    [cards[i], cards[j]] = [cards[j], cards[i]];
  }
  return cards;
}

function cardName(c) {
  return c === 1 ? 'A' : c === 11 ? 'J' : c === 12 ? 'Q' : c === 13 ? 'K' : String(c);
}

function total(cards) {
  let sum = 0, aces = 0;
  for (const c of cards) {
    if (c === 1) { aces++; sum += 11; }
    else sum += Math.min(c, 10);
  }
  while (sum > 21 && aces > 0) { sum -= 10; aces--; }
  return sum;
}

function blackjackStart(user, bet) {
  charge(user, bet);
  const d = deck();
  const game = { bet, deck: d, player: [d.pop(), d.pop()], dealer: [d.pop(), d.pop()] };
  blackjackSessions.set(cleanUser(user), game);
  if (total(game.player) === 21) {
    addCredits(user, bet * 3);
    blackjackSessions.delete(cleanUser(user));
    return blackjackText(user, game, true, `Blackjack. You won ${bet * 3} Credits.`);
  }
  return blackjackText(user, game, false, 'Blackjack started. Use /casino blackjack-hit or /casino blackjack-stand.');
}

function blackjackHit(user) {
  const game = blackjackSessions.get(cleanUser(user));
  if (!game) throw new Error('No blackjack game running. Use /casino blackjack first.');
  game.player.push(game.deck.pop());
  if (total(game.player) > 21) {
    blackjackSessions.delete(cleanUser(user));
    return blackjackText(user, game, true, `Bust at ${total(game.player)}.`);
  }
  return blackjackText(user, game, false, 'Card drawn.');
}

function blackjackStand(user) {
  const game = blackjackSessions.get(cleanUser(user));
  if (!game) throw new Error('No blackjack game running. Use /casino blackjack first.');
  while (total(game.dealer) < 17) game.dealer.push(game.deck.pop());
  const p = total(game.player), d = total(game.dealer);
  let msg = '';
  if (d > 21 || p > d) { addCredits(user, game.bet * 2); msg = `You won. ${p} vs dealer ${d}.`; }
  else if (p === d) { addCredits(user, game.bet); msg = `Push. Both ${p}.`; }
  else msg = `Dealer wins. ${d} vs ${p}.`;
  blackjackSessions.delete(cleanUser(user));
  return blackjackText(user, game, true, msg);
}

function blackjackText(user, game, reveal, msg) {
  const dealer = reveal ? game.dealer.map(cardName).join(', ') : `${cardName(game.dealer[0])}, Hidden`;
  const dealerTotal = reveal ? total(game.dealer) : '?';
  const b = balance(user);
  return `🃏 ${msg}\nPlayer: ${game.player.map(cardName).join(', ')} = ${total(game.player)}\nDealer: ${dealer} = ${dealerTotal}\nBalance: ${b.credits}`;
}

function shopItems() {
  return readJson(shopPath, []).filter(i => i.enabled !== false);
}

async function grantShopReward(interaction, item, user, cfg) {
  const explicitRoleId = String(item.discord_role_id || '').trim();
  const mappedRoleId = cfg.shop_role_mappings?.[item.name] || cfg.shop_role_mappings?.[String(item.name || '').toLowerCase()];
  const roleId = explicitRoleId || mappedRoleId;
  if ((item.reward_type || 'manual') !== 'discord_role' && !roleId) {
    return item.command_hint ? ` Fulfillment note: ${item.command_hint}` : '';
  }
  if (!roleId) {
    throw new Error('This item is configured as a Discord role reward, but no Discord Role ID is set.');
  }
  if (!interaction.guild) {
    throw new Error('Discord role rewards must be purchased inside the configured server, not DMs.');
  }
  const member = await interaction.guild.members.fetch(interaction.user.id);
  await member.roles.add(roleId);
  appendJsonArray(shopAuditPath, {
    at: new Date().toISOString(),
    action: 'discord_role_granted',
    item_id: item.id || '',
    item_name: item.name || '',
    bank_user: user,
    discord_user_id: interaction.user.id,
    discord_username: interaction.user.tag || interaction.user.username,
    guild_id: interaction.guild.id,
    role_id: roleId,
    role_name: item.discord_role_name || '',
    price: item.price || 0
  });
  return ` Discord role granted${item.discord_role_name ? `: ${item.discord_role_name}` : ''}.`;
}

const commands = [
  new SlashCommandBuilder().setName('help').setDescription('Show Credits casino and shop commands.'),
  new SlashCommandBuilder().setName('credits').setDescription('Credits bank commands.')
    .addSubcommand(s => s.setName('balance').setDescription('Show your balance')),
  new SlashCommandBuilder().setName('casino').setDescription('Play casino games with Credits.')
    .addSubcommand(s => s.setName('slots').setDescription('Black and white slots').addIntegerOption(o => o.setName('bet').setDescription('Bet').setRequired(true)))
    .addSubcommand(s => s.setName('roulette').setDescription('Roulette').addIntegerOption(o => o.setName('bet').setDescription('Bet').setRequired(true)).addStringOption(o => o.setName('choice').setDescription('red, black, green, odd, even, or 0-36').setRequired(true)))
    .addSubcommand(s => s.setName('blackjack').setDescription('Start blackjack').addIntegerOption(o => o.setName('bet').setDescription('Bet').setRequired(true)))
    .addSubcommand(s => s.setName('blackjack-hit').setDescription('Hit current blackjack hand'))
    .addSubcommand(s => s.setName('blackjack-stand').setDescription('Stand current blackjack hand')),
  new SlashCommandBuilder().setName('shop').setDescription('Server shop commands.')
    .addSubcommand(s => s.setName('list').setDescription('List shop items'))
    .addSubcommand(s => s.setName('buy').setDescription('Buy a shop item by exact name').addStringOption(o => o.setName('item').setDescription('Exact item name').setRequired(true))),
  new SlashCommandBuilder().setName('admin').setDescription('Admin helpers.')
    .setDefaultMemberPermissions(PermissionFlagsBits.ManageGuild)
    .addSubcommand(s => s.setName('sync-commands').setDescription('Re-register slash commands')),
].map(c => c.toJSON());

async function registerCommands(cfg) {
  const rest = new REST({ version: '10' }).setToken(cfg.token);
  await rest.put(Routes.applicationGuildCommands(cfg.client_id, cfg.guild_id), { body: commands });
}

async function main() {
  const cfg = config();
  if (process.argv.includes('--register')) {
    await registerCommands(cfg);
    console.log('Discord slash commands registered.');
    return;
  }
  const client = new Client({ intents: [GatewayIntentBits.Guilds, GatewayIntentBits.GuildMembers] });
  client.once('ready', async () => {
    console.log(`Discord bot logged in as ${client.user.tag}`);
    try { await registerCommands(cfg); console.log('Discord slash commands registered.'); }
    catch (e) { console.error('Slash command registration failed:', e.mesadmin); }
  });
  client.on('interactionCreate', async interaction => {
    if (!interaction.isChatInputCommand()) return;
    try {
      const user = bankUserForInteraction(interaction);
      if (interaction.commandName === 'help') {
        await interaction.reply('Commands: /credits balance, /casino slots, /casino roulette, /casino blackjack, /casino blackjack-hit, /casino blackjack-stand, /shop list, /shop buy.');
      } else if (interaction.commandName === 'credits') {
        const b = balance(user);
        await interaction.reply(`${b.username} has ${b.credits} ${cfg.currency_name || 'Credits'}.`);
      } else if (interaction.commandName === 'casino') {
        const sub = interaction.options.getSubcommand();
        const bet = interaction.options.getInteger('bet');
        const text = sub === 'slots' ? playSlots(user, bet)
          : sub === 'roulette' ? playRoulette(user, bet, interaction.options.getString('choice'))
          : sub === 'blackjack' ? blackjackStart(user, bet)
          : sub === 'blackjack-hit' ? blackjackHit(user)
          : blackjackStand(user);
        await interaction.reply(text);
      } else if (interaction.commandName === 'shop') {
        const sub = interaction.options.getSubcommand();
        if (sub === 'list') {
          const rows = shopItems();
          await interaction.reply(rows.length ? rows.map(i => { const reward = i.reward_type === 'discord_role' ? `\nReward: Discord role${i.discord_role_name ? ` (${i.discord_role_name})` : ''}` : ''; return `**${i.name}** - ${i.price} ${cfg.currency_name || 'Credits'}${reward}\n${i.description || i.command_hint || ''}`; }).join('\n\n') : 'No shop items configured.');
        } else {
          const wanted = interaction.options.getString('item');
          const item = shopItems().find(i => i.name.toLowerCase() === wanted.toLowerCase());
          if (!item) throw new Error('Shop item not found. Use /shop list.');
          charge(user, item.price);
          const extra = await grantShopReward(interaction, item, user, cfg);
          await interaction.reply(`Purchased **${item.name}** for ${item.price} ${cfg.currency_name || 'Credits'}.${extra} Balance: ${balance(user).credits}`);
        }
      } else if (interaction.commandName === 'admin') {
        await registerCommands(cfg);
        await interaction.reply({ content: 'Slash commands synced.', ephemeral: true });
      }
    } catch (e) {
      await interaction.reply({ content: `Error: ${e.mesadmin}`, ephemeral: true }).catch(() => {});
    }
  });
  await client.login(cfg.token);
}

main().catch(err => {
  console.error(err);
  process.exit(1);
});
