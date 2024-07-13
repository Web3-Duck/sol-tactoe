use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer as SplTransfer};
use num_derive::*;

declare_id!("J2hD3qwZMr77ZPZWkbT5abK4TwgnVnPrxSx89Fo5Sxng");

pub const WIN_PATTERSN: [[usize; 3]; 8] = [
    [0, 1, 2],
    [3, 4, 5],
    [6, 7, 8], // 横排
    [0, 3, 6],
    [1, 4, 7],
    [2, 5, 8], // 竖排
    [0, 4, 8],
    [2, 4, 6], // 对角线
];

pub fn get_random_number(balance: usize, offset: usize) -> Result<u8> {
    let clock = Clock::get()?;
    let slot_index = clock.slot;

    // 使用模运算将时隙索引转换为 0 - balance的随机数
    let random_number = slot_index % (balance + offset) as u64;

    Ok(random_number as u8)
}

pub fn shuffle_array(grid_order: [u8; 9]) -> Result<[u8; 9]> {
    // 使用 Fisher-Yates 洗牌算法打乱 grid_order 的顺序
    let mut shuffled_grid = grid_order;
    for i in (1..shuffled_grid.len()).rev() {
        let j = (Clock::get()?.slot % (i as u64 + 1)) as usize;
        shuffled_grid.swap(i, j);
    }

    Ok(shuffled_grid)
}

pub const ARRAY: [u8; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];

#[program]
pub mod tactoe {
    use super::*;
    pub fn setup_game(ctx: Context<SetupGame>) -> Result<()> {
        let game: &mut Box<Account<Game>> = &mut ctx.accounts.game;
        game.player = ctx.accounts.player_one.key();
        game.my_balance = 100;
        game.op_balance = 100;
        game.current_index = 0;
        game.rest_grid = shuffle_array(ARRAY).unwrap();
        game.can_get_reward = false;
        Ok(())
    }

    pub fn play(ctx: Context<Play>, amount: u8) -> Result<()> {
        let game = &mut ctx.accounts.game;
        if ctx.accounts.player.key() != game.player {
            return Err(TicTacToeError::InvalidPlayer.into());
        }
        game.play(amount)
    }

    pub fn get_reward(ctx: Context<GetReward>) -> Result<()> {
        let game = &mut ctx.accounts.game;
        if ctx.accounts.player.key() != game.player {
            return Err(TicTacToeError::InvalidPlayer.into());
        }

        if !game.can_get_reward {
            return Err(TicTacToeError::RewardAlreadyTaken.into());
        }

        game.can_get_reward = false;

        let destination = &ctx.accounts.to_ata;
        let source = &ctx.accounts.pda_ata;
        let token_program = &ctx.accounts.token_program;
        let authority = &ctx.accounts.pda;
        let signer = &ctx.accounts.player;
        let bump = &[ctx.bumps.pda];
        let signer_key = signer.key();

        // Transfer tokens from taker to initializer
        let cpi_accounts = SplTransfer {
            from: source.to_account_info().clone(),
            to: destination.to_account_info().clone(),
            authority: authority.to_account_info().clone(),
        };
        let cpi_program = token_program.to_account_info();

        let seeds: &[&[u8]] = &[b"vault".as_ref(), signer_key.as_ref(), bump];
        let signer_seeds = &[&seeds[..]];

        token::transfer(
            CpiContext::new(cpi_program, cpi_accounts).with_signer(signer_seeds),
            100,
        )?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct SetupGame<'info> {
    #[account(init, payer = player_one, space = 240 + 8)]
    pub game: Box<Account<'info, Game>>,
    #[account(mut)]
    pub player_one: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Play<'info> {
    #[account(mut)]
    pub game: Box<Account<'info, Game>>,
    pub player: Signer<'info>,
}

#[derive(Accounts)]
pub struct GetReward<'info> {
    #[account(mut)]
    pub game: Box<Account<'info, Game>>,
    pub player: Signer<'info>,
    #[account(mut)]
    pub pda_ata: Account<'info, TokenAccount>,
    #[account(mut)]
    pub to_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"vault".as_ref(), player.key().as_ref() ],
        bump
    )]
    pub pda: SystemAccount<'info>,
    pub token_program: Program<'info, Token>,
}

#[account]
#[derive(Default)]
pub struct Game {
    player: Pubkey,     // 64
    board: [Grid; 9],   // 3 * 3 * 32
    state: GameState,   // 32 + 1
    my_balance: u8,     // 8
    op_balance: u8,     // 8
    current_index: u8,  // 8
    rest_grid: [u8; 9], // 8
    can_get_reward: bool,
}

#[derive(Default, AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Grid {
    my_bid: u8,
    op_bid: u8,
    flag: Option<Sign>,
}

impl Game {
    pub fn is_active(&self) -> bool {
        self.state == GameState::Active
    }

    pub fn play(&mut self, amount: u8) -> Result<()> {
        if !self.is_active() {
            return Err(TicTacToeError::GameAlreadyOver.into());
        }
        if amount > self.my_balance {
            return Err(TicTacToeError::OverBalance.into());
        }

        let current_grid = self.rest_grid[self.current_index as usize];

        match self.board[current_grid as usize].flag {
            Some(_) => return Err(TicTacToeError::TileAlreadySet.into()),
            None => {
                let random_number = get_random_number(self.op_balance.into(), 1).unwrap();
                self.board[current_grid as usize].my_bid = amount;
                self.board[current_grid as usize].op_bid = random_number as u8;

                let flag = if amount > random_number as u8 {
                    Sign::X
                } else {
                    Sign::O
                };
                self.board[current_grid as usize].flag = Some(flag);
                self.my_balance -= amount;
                self.op_balance -= random_number as u8;
            }
        }

        self.update_state();
        Ok(())
    }

    fn is_winning_trio(&self, trio: [usize; 3]) -> bool {
        let [first, second, third] = trio;
        self.board[first].flag.is_some()
            && self.board[first].flag == self.board[second].flag
            && self.board[first].flag == self.board[third].flag
    }

    fn is_winning_over_equal_5(&self) -> (u8, u8) {
        let mut count_x = 0;
        let mut count_o = 0;

        for i in 0..=8 {
            if self.board[i].flag.is_some() {
                if self.board[i].flag == Some(Sign::X) {
                    count_x += 1;
                } else {
                    count_o += 1;
                }
            }
        }
        (count_x, count_o)
    }

    fn update_state(&mut self) {
        for pattern in WIN_PATTERSN.iter() {
            if self.is_winning_trio(*pattern) {
                if self.board[pattern[0]].flag == Some(Sign::X) {
                    self.state = GameState::Won
                } else {
                    self.state = GameState::Lose
                }
                self.can_get_reward = true;
                return;
            }
        }

        let (count_x, count_o) = self.is_winning_over_equal_5();

        if count_x == 5 {
            self.state = GameState::Won;
            self.can_get_reward = true;
            return;
        } else if count_o == 5 {
            self.state = GameState::Lose;
            self.can_get_reward = true;
            return;
        }
        self.current_index += 1;
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::Active
    }
}

#[error_code]
pub enum TicTacToeError {
    TileOutOfBounds,
    TileAlreadySet,
    GameAlreadyOver,
    OverBalance,
    NotplayerTurn,
    InvalidPlayer,
    RewardAlreadyTaken,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum GameState {
    Active,
    Won,
    Lose,
}

#[derive(AnchorSerialize, AnchorDeserialize, Copy, Clone, PartialEq, Eq, FromPrimitive)]
pub enum Sign {
    X, // my
    O, // bot
}
